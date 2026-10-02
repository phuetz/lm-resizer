// LM Resizer: Pi and Oh My Pi tool_call adapter (legacy-pi-compat).
export default function (pi) {
  pi.on("tool_call", async (event, context) => {
    if (event.toolName !== "bash" || typeof event.input?.command !== "string") return;
    try {
      const result = await pi.exec("lm-resizer", ["hook", "check", "--agent", "pi", event.input.command], { timeout: 2000, signal: context?.signal });
      if (result.killed || result.code !== 0) return;
      const report = JSON.parse(result.stdout);
      if (report.changed && typeof report.rewritten === "string") event.input.command = report.rewritten;
    } catch { /* The host retains its original command. */ }
  });
}
