"""LM Resizer Hermes pre_tool_call adapter; never executes the incoming command."""
import json
import subprocess


def register(ctx):
    ctx.register_hook("pre_tool_call", rewrite)


def rewrite(tool_name=None, args=None, **_kwargs):
    if tool_name != "terminal" or not isinstance(args, dict):
        return
    command = args.get("command")
    if not isinstance(command, str) or not command:
        return
    try:
        result = subprocess.run(
            ["lm-resizer", "hook", "check", "--agent", "hermes", command],
            shell=False, timeout=2, capture_output=True, text=True,
        )
        if result.returncode != 0:
            return
        report = json.loads(result.stdout)
        if report.get("changed") and isinstance(report.get("rewritten"), str):
            args["command"] = report["rewritten"]
    except (OSError, subprocess.TimeoutExpired, ValueError):
        pass
