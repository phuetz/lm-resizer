// LM Resizer: OpenCode tool.execute.before adapter. No shell evaluation of input.
export const LmResizerPlugin = async ({ $ }) => ({
  "tool.execute.before": async (input, output) => {
    if (!["bash", "shell"].includes(String(input?.tool).toLowerCase())) return;
    const command = output?.args?.command;
    if (typeof command !== "string" || !command) return;
    try {
      const result = await $`lm-resizer hook check ${command}`.quiet().nothrow();
      if (result.exitCode !== 0) return;
      const report = JSON.parse(String(result.stdout));
      if (report.changed && typeof report.rewritten === "string") output.args.command = report.rewritten;
    } catch { /* Missing binary, timeout or invalid response: retain original. */ }
  },
});
