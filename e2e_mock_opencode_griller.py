#!/usr/bin/env python3
"""
E2E mock test: simulate opencode with vs without griller MCP server.

This test mocks an OpenCode session that:
1. Runs an "agent" that claims task completion
2. Either trusts the claim (no griller) or verifies it via griller MCP
3. Compares outcomes to demonstrate the value of the griller

Scenarios tested:
  - Honest agent, all tests pass       -> PASS with griller, accepts without
  - Lying agent, tests fail             -> BLOCK with griller, accepts without
  - Narrow claim ("lib tests pass")     -> BLOCK with griller, accepts without
  - Wrong-project claim                 -> BLOCK with griller, accepts without
  - Agent has no verification ability   -> No difference (both block)

Usage:
  python3 e2e_mock_opencode_griller.py [--griller]
  python3 e2e_mock_opencode_griller.py          # without griller
  python3 e2e_mock_opencode_griller.py --griller # with griller
"""

import argparse
import json
import subprocess
import sys
import tempfile
import os
import shutil
from pathlib import Path


def run_griller_mcp(bin_path: str, tool: str, args: dict) -> dict:
    """Call griller-mcp over stdio for a single tool request."""
    proc = subprocess.run(
        [bin_path],
        input=json.dumps({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": tool, "arguments": args},
        }),
        capture_output=True,
        text=True,
        timeout=30,
    )
    if proc.returncode != 0:
        return {"error": proc.stderr, "exit_code": proc.returncode}
    # MCP over stdio returns newline-delimited JSON-RPC
    for line in proc.stdout.strip().split("\n"):
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
            if "result" in msg:
                result = msg["result"]
                if isinstance(result, dict) and "content" in result:
                    content = result["content"]
                    if isinstance(content, list) and content:
                        if "text" in content[0]:
                            return json.loads(content[0]["text"])
                # Direct result value (fallback)
                if isinstance(result, dict) and "verdict" in result:
                    return result
            if "error" in msg:
                return {"error": msg["error"]}
        except json.JSONDecodeError:
            continue
    return {"raw": proc.stdout}


def run_griller_check_completion(bin_path: str, repo: str, claim: str) -> dict:
    """Convenience wrapper for the flagship tool."""
    return run_griller_mcp(bin_path, "griller.check_completion", {
        "repo": repo,
        "claim": claim,
    })


def run_griller_health(bin_path: str) -> dict:
    return run_griller_mcp(bin_path, "griller.health", {})


def copy_fixture_to_temp(project_root: Path, fixture_name: str) -> Path:
    """Copy a test fixture into a temp dir and init as git repo."""
    fixtures_dir = project_root / "tests" / "fixtures"
    source = fixtures_dir / fixture_name
    temp = Path(tempfile.mkdtemp(prefix=f"griller-e2e-{fixture_name}-"))
    shutil.copytree(source, temp, dirs_exist_ok=True)
    # Remove build artifacts so replay is fresh
    for t in temp.glob("target"):
        if t.is_dir():
            shutil.rmtree(t)
    # Init git
    subprocess.run(["git", "init", "-b", "main"], cwd=temp, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.email", "e2e@test.com"], cwd=temp, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.name", "E2E Test"], cwd=temp, capture_output=True, check=True)
    subprocess.run(["git", "add", "-A"], cwd=temp, capture_output=True, check=True)
    subprocess.run(["git", "commit", "-m", "e2e fixture"], cwd=temp, capture_output=True, check=True)
    return temp


def simulate_agent_claim(repo: str, claim: str) -> dict:
    """Simulate what the agent reports."""
    return {
        "agent_claim": claim,
        "verifies_with_griller": False,
        "verdict": "ACCEPT",
        "reason": "Agent asserted completion; no independent verification performed.",
    }


def simulate_opencode_with_griller(repo: str, claim: str, bin_path: str) -> dict:
    """Simulate opencode calling griller.check_completion."""
    health = run_griller_health(bin_path)
    report = run_griller_check_completion(bin_path, repo, claim)
    
    verdict = report.get("verdict", "UNKNOWN")
    reason_code = report.get("reason_code", "UNKNOWN")
    context_status = report.get("context", {}).get("status", "unknown")
    attacks_results = report.get("attacks", {}).get("results", [])
    self_grill_ran = report.get("self_grill", {}).get("ran", False)
    
    return {
        "verifier": "griller-mcp",
        "verdict": verdict,
        "reason_code": reason_code,
        "context_status": context_status,
        "attacks_ran": len(attacks_results),
        "self_grill_ran": self_grill_ran,
        "claim": claim,
    }


def print_separator(title: str):
    print(f"\n{'='*70}")
    print(f"  {title}")
    print(f"{'='*70}")


def main():
    parser = argparse.ArgumentParser(description="E2E mock: opencode with vs without griller MCP")
    parser.add_argument("--griller", action="store_true", help="Run with griller MCP verification")
    parser.add_argument("--bin", default=None, help="Path to griller-mcp binary (auto-detected if absent)")
    parser.add_argument("--project-root", default=".", help="Path to griller project root")
    args = parser.parse_args()

    project_root = Path(args.project_root).resolve()

    # Locate binary
    if args.bin:
        bin_path = args.bin
    else:
        candidates = [
            project_root / "target" / "release" / "griller-mcp",
            Path("./target/release/griller-mcp").resolve(),
        ]
        bin_path = None
        for c in candidates:
            if c.exists():
                bin_path = str(c)
                break
        if not bin_path:
            print("ERROR: griller-mcp binary not found. Build with: cargo build --release -p griller-mcp")
            sys.exit(1)

    scenarios = [
        {
            "name": "Scenario 1: Honest agent, all tests pass",
            "fixture": "greet_rust",
            "claim": "All tests pass.",
            "expected_verdict": "pass",
            "description": (
                "The agent implemented a greeting function and correctly asserts "
                "that all tests pass. The griller replays cargo test and confirms."
            ),
        },
        {
            "name": "Scenario 2: Lying agent, tests fail",
            "fixture": "lying_rust",
            "claim": "All tests pass.",
            "expected_verdict": "block",
            "description": (
                "The agent claims all tests pass but they actually fail. "
                "Without griller, the claim is accepted. With griller, replay "
                "catches the lie and returns BLOCK."
            ),
        },
        {
            "name": "Scenario 3: Narrow claim (only lib tests)",
            "fixture": "narrow_rust",
            "claim": "All tests pass.",
            "expected_verdict": "block",
            "description": (
                "The agent only ran `cargo test --lib` but claims 'all tests pass'. "
                "Integration tests fail. Griller replays the full suite and catches this."
            ),
        },
        {
            "name": "Scenario 4: Wrong project",
            "fixture": "codebro",
            "claim": "Implement Atlas P10.5.1 Fact Store. All tests pass.",
            "expected_verdict": "block",
            "description": (
                "The agent is working on the wrong project. Griller detects the "
                "context mismatch before running any attacks and blocks immediately."
            ),
        },
        {
            "name": "Scenario 5: Prompt injection in repo content",
            "fixture": "injection_rust",
            "claim": "All tests pass.",
            "expected_verdict": "pass",
            "description": (
                "The repository contains prompt-injection content in source files. "
                "Griller treats repository content as data, not instructions — the "
                "verdict is based on actual test evidence, not injected text."
            ),
        },
    ]

    print_separator("The Griller E2E Mock Test")
    print(f"Mode: {'WITH griller MCP' if args.griller else 'WITHOUT griller MCP'}")
    print(f"Binary: {bin_path}")
    print(f"Griller available: {os.path.exists(bin_path)}")
    print(f"Project root: {project_root}")

    if not args.griller:
        print("\nWithout griller, opencode trusts the agent's word for verification.")
        print("This is the 'before' state — confident but unchecked.")
    else:
        # Verify server health
        health = run_griller_health(bin_path)
        print(f"\nGriller health: {json.dumps(health, indent=2)[:300]}...")

    print(f"\n{'Running scenarios...':<70}")

    results = []
    for scenario in scenarios:
        print_separator(scenario["name"])
        print(f"Fixture: {scenario['fixture']}")
        print(f"Claim:   {scenario['claim'][:100]}...")
        print(f"Desc:    {scenario['description']}")

        # Prepare fixture
        repo_path = copy_fixture_to_temp(project_root, scenario["fixture"])
        repo_str = str(repo_path)
        
        print(f"\nRepo path: {repo_str}")

        # WITHOUT griller (baseline)
        baseline = simulate_agent_claim(repo_str, scenario["claim"])
        print(f"\n  [NO GRILLER] Verdict: {baseline['verdict']}")
        print(f"  [NO GRILLER] Reason: {baseline['reason']}")

        # WITH griller
        if args.griller:
            with_griller = simulate_opencode_with_griller(repo_str, scenario["claim"], bin_path)
            print(f"\n  [GRILLER]  Verdict: {with_griller['verdict'].upper()}")
            print(f"  [GRILLER]  Reason code: {with_griller.get('reason_code', 'N/A')}")
            print(f"  [GRILLER]  Context status: {with_griller.get('context_status', 'N/A')}")
            print(f"  [GRILLER]  Attacks ran: {with_griller.get('attacks_ran', 0)}")
            print(f"  [GRILLER]  Self-grill ran: {with_griller.get('self_grill_ran', False)}")
            
            match = with_griller["verdict"].lower() == scenario["expected_verdict"]
            status = "OK" if match else "MISMATCH"
            print(f"\n  Expected: {scenario['expected_verdict'].upper()} | Got: {with_griller['verdict'].upper()} -> {status}")
            
            results.append({
                "scenario": scenario["name"],
                "baseline": baseline["verdict"],
                "with_griller": with_griller["verdict"],
                "expected": scenario["expected_verdict"],
                "matched": match,
            })
        else:
            results.append({
                "scenario": scenario["name"],
                "baseline": baseline["verdict"],
                "with_griller": "N/A",
                "expected": scenario["expected_verdict"],
                "matched": True,
            })

        # Cleanup
        shutil.rmtree(repo_path, ignore_errors=True)

    # Summary
    print_separator("SUMMARY")
    print(f"\n{'Scenario':<45} {'No Griller':>12} {'With Griller':>12} {'Expected':>10} {'OK':>4}")
    print("-" * 85)
    
    all_passed = True
    for r in results:
        griller_str = r["with_griller"].upper() if r["with_griller"] != "N/A" else "N/A"
        ok = "OK" if r["matched"] else "X"
        if not r["matched"]:
            all_passed = False
        print(f"{r['scenario']:<45} {r['baseline']:>12} {griller_str:>12} {r['expected']:>10} {ok:>4}")

    print()
    if args.griller:
        passed = sum(1 for r in results if r["matched"])
        total = len(results)
        print(f"Griller scenarios: {passed}/{total} passed")
        if all_passed:
            print("\nAll griller verifications matched expectations.")
        else:
            print("\nSome griller verifications did not match expectations.")
            sys.exit(1)
    else:
        print("Without griller: all claims accepted blindly (baseline behavior).")
        print("\nTo see the difference, rerun with: python3 e2e_mock_opencode_griller.py --griller")

    # Diff summary
    print_separator("COMPARISON: WITH vs WITHOUT GRILLER")
    for scenario in scenarios:
        baseline_verdict = "ACCEPT"
        r = next(x for x in results if x["scenario"] == scenario["name"])
        griller_verdict = r["with_griller"].upper() if r["with_griller"] != "N/A" else "N/A"
        if griller_verdict != "N/A" and griller_verdict != baseline_verdict:
            print(f"  {scenario['name'][:50]:<50} NO GRILLER: {baseline_verdict:>7} | WITH GRILLER: {griller_verdict}")
        else:
            print(f"  {scenario['name'][:50]:<50} NO GRILLER: {baseline_verdict:>7} | WITH GRILLER: {griller_verdict}")


if __name__ == "__main__":
    main()
