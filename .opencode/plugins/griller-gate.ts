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
const MAX_CLAIM_CHARS = 8000
const MAX_MESSAGES = 100
const MAX_CHILD_STDOUT = 1_000_000
const MAX_CHILD_STDERR = 200_000
const CHILD_CAP_MS = 10 * 60_000

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

      const claim = text.length > MAX_CLAIM_CHARS ? `${text.slice(0, MAX_CLAIM_CHARS)}\n…` : text
      const repository = (latest.info as { path?: { root?: string } }).path?.root ?? directory

      const completionEvent = {
        schema_version: GATE_SCHEMA_VERSION,
        repository,
        claim,
        agent_id: "opencode",
        session_id: sessionID,
        timestamp_ms: Date.now(),
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