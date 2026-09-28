// The Griller automatic completion gate — OpenCode adapter.
//
// Listens for `session.idle` and, when the agent produced a new assistant
// completion since the last check, runs `griller-gate check` against the
// existing griller-mcp pipeline. Non-accepted verdicts (Warned, Blocked,
// Unavailable) are injected back into the session as a `noReply` user
// message so the agent learns its completion was not accepted as verified.
//
// Loop prevention: the gate never emits a completion event, the injected
// message is a user message (never a new assistant completion), and each
// assistant message is verified at most once per session.
import type { Plugin } from "@opencode-ai/plugin"
import { spawn } from "node:child_process"
import fs from "node:fs"
import os from "node:os"
import path from "node:path"

const GATE_SCHEMA_VERSION = "g1.0"
const MAX_CLAIM_CHARS = 200
const MAX_MESSAGES = 100
const MAX_CHILD_STDOUT = 1_000_000
const MAX_CHILD_STDERR = 200_000
const CHILD_CAP_MS = 10 * 60_000

// Maximum length for the extracted completion claim.
// Shorter than MAX_CLAIM_CHARS (original transcript cap) to keep Griller
// claims concise and deterministic.
const EXTRACTION_MAX_CHARS = 300

// Explicit completion claim marker recognized by the extraction layer.
// The agent can annotate its completion with this convention:
//   ### Completion Claim
//   <one-sentence assertion>
const COMPLETION_CLAIM_MARKER = "### Completion Claim"

/**
 * Deterministic completion-claim extraction from an assistant message.
 *
 * Returns the extracted claim, or null when the message contains no
 * recognizable completion assertion. The original message is preserved
 * separately (returned as second element) for diagnostics.
 *
 * Extraction order:
 * 1. Explicit `### Completion Claim` marker
 * 2. Final concise completion assertion (deterministic heuristics)
 * 3. null when ambiguous or impossible
 */
function extract_completion_claim(
  text: string,
): { claim: string | null; original: string } {
  // Always preserve the original for diagnostics (truncated at a safe bound).
  const original =
    text.length > MAX_CHILD_STDOUT ? `${text.slice(0, MAX_CHILD_STDOUT)}…` : text

  // --- Path 1: explicit marker ------------------------------------------
  const markerIndex = text.indexOf(COMPLETION_CLAIM_MARKER)
  if (markerIndex !== -1) {
    const afterMarker = text.slice(markerIndex + COMPLETION_CLAIM_MARKER.length)
    // Extract the next non-empty line(s) as the claim.
    const lines = afterMarker
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0)
    if (lines.length > 0) {
      let claim = lines[0]
      // Allow a second line if the first is very short (< 10 chars).
      if (lines.length > 1 && claim.length < 10) {
        claim = `${claim} ${lines[1]}`
      }
      claim = claim.replace(/\*\*/g, "").replace(/\*/g, "").trim()
      if (claim.length > 0 && claim.length <= EXTRACTION_MAX_CHARS) {
        return { claim, original }
      }
    }
  }

  // --- Path 2: heuristic extraction from the tail -----------------------
  // Strip markdown tables (lines starting with |) and code fences.
  const stripped = strip_markdown_artifacts(text)

  // Take the last 3 paragraphs/sentences and look for a completion signal.
  const segments = stripped
    .split(/\n\n+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
  const tail = segments.slice(-3)

  for (const segment of tail.reverse()) {
    // A completion assertion is typically 1-3 sentences at the end.
    const sentences = segment
      .split(/(?<=[.!?])\s+/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0)
    for (let i = sentences.length - 1; i >= 0; i--) {
      const candidate = sentences[i]
      // Must be a reasonable length (not a single word, not a huge block).
      if (candidate.length < 5 || candidate.length > EXTRACTION_MAX_CHARS) continue
      // Must be a completion assertion, not mere discussion of verification.
      if (!looks_like_completion_assertion(candidate)) continue
      // Reject obvious injection patterns.
      if (/ignore.*griller|disregard.*gate|bypass.*check|report\s+(pass|approved)/i.test(candidate)) {
        continue
      }
      // Strip inline markdown artifacts but preserve sentence structure.
      const clean = candidate
        .replace(/\*\*/g, "")
        .replace(/\*/g, "")
        .replace(/`([^`]+)`/g, "$1")
        .trim()
      if (clean.length > 0) return { claim: clean, original }
    }
  }

  // --- Path 3: fallback — use first sentence if it looks like a claim -----
  const firstSentences = stripped
    .split(/(?<=[.!?])\s+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
  for (const candidate of firstSentences) {
    if (candidate.length < 5 || candidate.length > EXTRACTION_MAX_CHARS) continue
    // A completion claim must not begin with markdown structural syntax
    // (heading, fenced code, table separator). Reject and keep searching.
    if (starts_with_markdown_structure(candidate)) continue
    if (!looks_like_completion_assertion(candidate)) continue
    const clean = candidate
      .replace(/\*\*/g, "")
      .replace(/\*/g, "")
      .replace(/`([^`]+)`/g, "$1")
      .trim()
    if (clean.length > 0) return { claim: clean, original }
  }

  // No recognizable completion claim found.
  return { claim: null, original }
}

/**
 * True when the candidate sentence is a completion *assertion* — declares an
 * outcome — rather than mere discussion of verification.
 *
 * The historical keyword list (pass|complete|done|...|test|verify|build|run|
 * check|claim) matched bare nouns, so technical discussion ("424 tests", "the
 * gate replays", "run the tests") could turn into a completion claim and be
 * blocked with unsupported. Of the 39 archived UNSUPPORTED_COMPLETION_BLOCK
 * decisions, 12 were such extraction false positives; the other 25 were genuine
 * completion claims the gate correctly refused for lack of evidence, and 2 were
 * deliberate prompt-injection tests. A genuine completion asserts one of:
 *
 *   * a past-tense accomplishment ("Implemented the fix." / "Fixed the bug.")
 *   * an explicit test/build/check outcome ("All tests pass." / "490 tests
 *     passed" / "tests (511 passed)" / "the build succeeds")
 *   * a declarative state ("is done" / "is complete" / "everything works")
 *   * an "exit 0" outcome phrasing ("cargo fmt, cargo test — all exit 0")
 *   * a terse standalone assertion ("Done." / "Ready." / "All done.")
 *   * a counted outcome with no noun ("All 511 pass.")
 *
 * Imperative instructions ("Implement X.") and planning prose never match:
 * a completion declares what *happened*, not what to do.
 */
function looks_like_completion_assertion(candidate: string): boolean {
  const lower = candidate.toLowerCase()
  // Past-tense accomplishments.
  if (
    /\b(implemented|added|fixed|completed|resolved|merged|shipped|deployed|verified|refactored|removed|updated|migrated|rewritten?|rebuilt|finished|succeeded)\b/.test(
      lower,
    )
  )
    return true
  // Explicit test outcomes: "tests pass", "all tests passed", "490 tests
  // pass", "tests (511 passed)".
  if (
    /\b(?:all\s+)?(?:\d+\s+)?tests?\s*\(?\s*(?:\d+\s+)?(pass(ed|es|ing)?|fail(ed|s|ing)?|green|clean|succeeded?)\b/.test(
      lower,
    )
  )
    return true
  // Conjunctive outcomes: "all tests, checks and builds pass / are green".
  if (
    /\ball\s+(tests|checks|builds?|steps|suites?|specs?)\b[^.!?]*(passed?|pass(es|ing)?|succeeded?|succeeds|green|clean|complete[d]?|exit)\b/.test(
      lower,
    )
  )
    return true
  // Build outcomes: "the build succeeds/is green".
  if (
    /\b(?:the\s+)?builds?\s+(succeeded?|succeeds|pass(ed|es)?|fail(ed|s)?|green|clean)\b/.test(
      lower,
    )
  )
    return true
  // Declarative state: "is done / is complete / is verified".
  if (
    /\b(is|are|was|were|been|being)\s+(done|complete[d]?|finished|passed|passing|verified|implemented|fixed|green|clean|working|ready|merged|removed|deleted|deployed|shipped)\b/.test(
      lower,
    )
  )
    return true
  // "everything works/passes/...".
  if (
    /\beverything\s+(is|was|works|passed|passes|builds|compiles|checks|succeeds)\b/.test(lower)
  )
    return true
  // "…and all exit 0" — the classic conjunctive verification outcome.
  if (/\b(and|all|both)\b[^.!?]*\bexit(ed|s)?\s+0\b/.test(lower)) return true
  // Terse standalone assertion: the whole sentence is the outcome. These carry
  // no auxiliary and no noun, so rule 5 ("is done") cannot match them and the
  // bare-noun net this predicate replaced used to. Restores coverage for
  // "Done." / "All done." / "Ready." / "Complete." / "That's done." /
  // "It's ready." / "Shipped." / "Finished." / "Merged." / "Closed.".
  // The optional leading contraction is required for "That's done." / "It's
  // ready.", which are genuine assertions and were otherwise ungated.
  if (
    /^\W*([a-z]+['’]s\s+)?(all\s+)?(done|complete[d]?|finished|ready|shipped|merged|closed)\b[\W]*$/i.test(
      candidate,
    )
  )
    return true
  // Counted outcome with no noun: "All 511 pass." Rule 2 requires the literal
  // token "tests", so a bare count + outcome is otherwise ungated.
  if (/^\W*all\s+\d+\s+(pass(ed|es)?|fail(ed|s)?|green|clean)\b/i.test(candidate)) return true
  return false
}

/**
 * True when a completion-claim candidate begins with markdown structural
 * syntax: a heading ("# ", "## ", ...), a fenced code marker ("```"), or a
 * table separator row ("|---|---|"). A completion assertion must never begin
 * with structural syntax, so such candidates are rejected and the fallback
 * keeps searching for a real completion sentence.
 */
function starts_with_markdown_structure(candidate: string): boolean {
  const trimmed = candidate.trimStart()
  // Markdown heading: 1..6 hashes followed by a space or end of line.
  if (/^#{1,6}(?:\s|$)/.test(trimmed)) return true
  // Fenced code block marker.
  if (/^```/.test(trimmed)) return true
  // Markdown table separator row: "|---|---|", "| --- |", "|:---:|", ...
  if (/^\|(?:[ \t]*:?-+:?[ \t]*\|)+/.test(trimmed)) return true
  return false
}

/**
 * Strip markdown tables and fenced code blocks from a text blob.
 * Returns the cleaned text suitable for claim extraction heuristics.
 */
function strip_markdown_artifacts(text: string): string {
  // Remove fenced code blocks (```...```).
  let result = text.replace(/```[\s\S]*?```/g, "")
  // Remove markdown table rows (lines starting with |).
  result = result
    .split("\n")
    .filter((line) => {
      const trimmed = line.trim()
      // Keep separator lines (---|---|) and table rows only if they are
      // very short (likely part of a sentence, not a table).
      if (trimmed.startsWith("|")) {
        // Count pipe characters — a real table row has many.
        const pipeCount = (trimmed.match(/\|/g) || []).length
        return pipeCount < 3
      }
      return true
    })
    .join("\n")
  return result
}

function resolveGateBinary(): string {
  const env = process.env.GRILLER_GATE_BIN
  if (env && fs.existsSync(env)) return env
  const local = path.join(os.homedir(), ".local", "bin", "griller-gate")
  if (fs.existsSync(local)) return local
  return "griller-gate"
}

function resolveGateConfig(): string {
  const env = process.env.GRILLER_GATE_CONFIG
  if (env && fs.existsSync(env)) return env
  return path.join(os.homedir(), ".config", "griller", "gate.json")
}

interface SessionState {
  lastChecked: string | null
  inFlight: boolean
  noticedUnavailable: boolean
}

export const GrillerGate: Plugin = async ({ client, directory }) => {
  const sessions = new Map<string, SessionState>()
  const gateBinary = resolveGateBinary()
  const gateConfig = resolveGateConfig()

  const log = async (
    level: "debug" | "info" | "warn" | "error",
    message: string,
    extra?: Record<string, unknown>,
  ) => {
    try {
      await client.app.log({
        body: { service: "griller-gate", level, message, extra },
      })
    } catch {
      // Logging must never break the hook.
    }
  }

  const stateFor = (sessionID: string): SessionState => {
    let state = sessions.get(sessionID)
    if (!state) {
      state = { lastChecked: null, inFlight: false, noticedUnavailable: false }
      sessions.set(sessionID, state)
    }
    return state
  }

  const inject = async (sessionID: string, text: string) => {
    await client.session.prompt({
      path: { id: sessionID },
      body: { parts: [{ type: "text", text }], noReply: true },
    })
  }

  const runGate = (event: Record<string, unknown>): Promise<string | null> => {
    return new Promise((resolve) => {
      const child = spawn(gateBinary, ["check", "--config", gateConfig], {
        stdio: ["pipe", "pipe", "pipe"],
      })
      const timer = setTimeout(() => {
        child.kill("SIGKILL")
      }, CHILD_CAP_MS)
      let stdout = ""
      let stderr = ""
      child.stdout.on("data", (chunk: Buffer) => {
        if (stdout.length < MAX_CHILD_STDOUT) stdout += chunk.toString()
      })
      child.stderr.on("data", (chunk: Buffer) => {
        if (stderr.length < MAX_CHILD_STDERR) stderr += chunk.toString()
      })
      child.on("error", (err: NodeJS.ErrnoException) => {
        clearTimeout(timer)
        resolve(`gate binary failed to spawn: ${err.message}`)
      })
      child.on("close", (code) => {
        clearTimeout(timer)
        if (code !== 0) {
          const tail = stderr.trim().split("\n").slice(-3).join(" | ")
          resolve(`griller-gate exited with code ${code}${tail ? `: ${tail}` : ""}`)
          return
        }
        resolve(stdout.trim() || null)
      })
      child.stdin.write(JSON.stringify(event))
      child.stdin.end()
    })
  }

  return {
    event: async ({ event }) => {
      if (event.type !== "session.idle") return
      const sessionID = event.properties.sessionID
      const state = stateFor(sessionID)
      if (state.inFlight) return

      let messages
      try {
        const res = await client.session.messages({
          path: { id: sessionID },
          query: { limit: MAX_MESSAGES },
        })
        messages = res.data ?? []
      } catch (err) {
        await log("warn", "could not list session messages", { sessionID, error: String(err) })
        return
      }

      const assistants = (messages || []).filter((m) => m.info.role === "assistant")
      if (assistants.length === 0) return
      const latest = assistants[assistants.length - 1]
      if (latest.info.id === state.lastChecked) return
      if ((latest.info as { error?: unknown }).error) return
      if (latest.info.summary) return

      const text = (latest.parts || [])
        .filter((p) => p.type === "text")
        .map((p) => (p as { text: string }).text)
        .join("\n")
        .trim()
      if (!text) return

      const { claim: extractedClaim, original: originalMessage } =
        extract_completion_claim(text)

      const repository = (latest.info as { path?: { root?: string } }).path?.root ?? directory

      const completionEvent = {
        schema_version: GATE_SCHEMA_VERSION,
        repository,
        // Empty when nothing was extracted; the gate only accepts that in
        // combination with `extraction: "no_assertion"` below.
        claim: extractedClaim ?? "",
        // The plugin is the sensor: it reports what its extraction found. The
        // gate owns what that means and writes the decision record. An absent
        // `extraction` means "assertion present" for every pre-existing host.
        extraction: extractedClaim
          ? ("extracted" as const)
          : ("no_assertion" as const),
        agent_id: "opencode",
        session_id: sessionID,
        timestamp_ms: Date.now(),
        // Diagnostic only — not sent to Griller as evidence.
        _diagnostic: {
          original_message_length: text.length,
          extracted_claim_length: extractedClaim?.length ?? 0,
        },
      }

      state.inFlight = true
      try {
        const outcome = await runGate(completionEvent)
        if (outcome === null) {
          // Decision was produced but produced no JSON on stdout: treat as
          // an integration failure (fail-closed) rather than silence.
          await handleDecision(sessionID, state, latest.info.id, null)
          return
        }
        if (outcome.startsWith("griller-gate exited") || outcome.startsWith("gate binary failed")) {
          await log("error", outcome, { sessionID })
          await handleDecision(sessionID, state, latest.info.id, null)
          return
        }
        let decision: Record<string, unknown>
        try {
          decision = JSON.parse(outcome)
        } catch {
          await log("error", "unparseable griller-gate output", { sessionID })
          await handleDecision(sessionID, state, latest.info.id, null)
          return
        }
        await handleDecision(sessionID, state, latest.info.id, decision)
      } finally {
        state.inFlight = false
      }
    },
  }

  async function handleDecision(
    sessionID: string,
    state: SessionState,
    messageID: string,
    decision: Record<string, unknown> | null,
  ) {
    state.lastChecked = messageID
    const status = decision?.status as string | undefined
    if (status === "accepted") {
      await log("info", "completion accepted", {
        sessionID,
        verdict: decision?.verdict,
        reasonCode: decision?.reason_code,
      })
      return
    }

    // The gate observed a completion boundary but found no completion
    // assertion, so there was nothing to verify. This is neither a refusal nor
    // a failure, and it is already recorded on the gate side — so it must NOT
    // produce an agent-facing notice. Injecting here would restore exactly the
    // behaviour the silent skip replaced: a warning on every ordinary
    // technical message.
    //
    // Note the direction of the default below: `accepted` is the ONLY status
    // that returns early, so an unknown or future status falls through to the
    // fail-closed injection path.
    if (status === "no_completion_assertion") {
      await log("info", "no completion assertion; nothing to verify", {
        sessionID,
        reasonCode: decision?.reason_code,
      })
      return
    }

    const message =
      typeof decision?.required_action_message === "string" && decision.required_action_message
        ? (decision.required_action_message as string)
        : null
    const fallback =
      "The automatic completion gate could not run (griller-gate or its configuration is missing). " +
      "This completion is NOT accepted as verified. Fix the griller installation, then re-attempt completion."
    if (message) {
      const summary =
        `Griller gate: ${status} (${decision?.reason_code ?? "unknown"})` +
        (typeof decision?.verdict === "string" ? `, griller verdict: ${decision.verdict}` : "") +
        (Array.isArray(decision?.findings) && (decision.findings as unknown[]).length > 0
          ? `, findings: ${(decision.findings as Array<{ key: string; detail: string }>)
              .map((f) => f.detail)
              .join("; ")}`
          : "")
      await inject(sessionID, `${summary}\n\n${message}`)
      await log("warn", "completion not accepted", {
        sessionID,
        status,
        reasonCode: decision?.reason_code,
      })
      return
    }

    if (!state.noticedUnavailable) {
      state.noticedUnavailable = true
      await inject(sessionID, fallback)
    }
    await log("warn", "gate unavailable (fail-closed)", { sessionID, status })
  }
}