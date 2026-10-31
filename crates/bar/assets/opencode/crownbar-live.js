// Installed by `crownbar install`. Forwards opencode's lifecycle to the
// CrownOS bar under Claude Code's hook names, which crownbar reads.
const EXE = "{exe}";

const LIFECYCLE = {
  "session.created": "SessionStart",
  "session.idle": "Stop",
  "session.error": "StopFailure",
  "session.compacted": "PreCompact",
  "session.deleted": "SessionEnd",
  "permission.asked": "PermissionRequest",
  "permission.replied": "PostToolUse",
};

export const CrownbarLive = async ({ $, directory }) => {
  const report = (session, name, extra = {}) =>
    $`${EXE} hook opencode ${JSON.stringify({
      session_id: session ?? "opencode",
      cwd: directory,
      hook_event_name: name,
      ...extra,
    })}`
      .quiet()
      .nothrow();

  return {
    "chat.message": async (input) => report(input?.sessionID, "UserPromptSubmit"),
    "tool.execute.before": async (input, output) =>
      report(input?.sessionID, "PreToolUse", {
        tool_name: input?.tool,
        tool_input: output?.args ?? {},
      }),
    "tool.execute.after": async (input) => report(input?.sessionID, "PostToolUse"),
    event: async ({ event }) => {
      const name = LIFECYCLE[event.type];
      if (!name) return;
      const session = event.properties?.sessionID ?? event.properties?.info?.id;
      await report(session, name);
    },
  };
};
