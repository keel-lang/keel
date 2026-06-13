---
name: graphify-explorer
description: Use for graph-guided codebase exploration, architecture mapping, feature tracing, or review prep when graphify-out/GRAPH_REPORT.md exists. Start from graphify communities before broad file scanning.
---

# Graphify Explorer

Use this skill when a task needs codebase exploration and `graphify-out/GRAPH_REPORT.md` exists in the project root.

## Workflow

1. Read `graphify-out/GRAPH_REPORT.md` before broad directory or file scans.
2. Identify the community hubs and node sections that match the task topic.
3. Navigate directly to files referenced by those communities.
4. Read only the graph-relevant files first, then expand with targeted `rg` searches if the graph leaves gaps.
5. Treat `graphify-out/` as generated navigation data unless the user is specifically asking about graphify itself.

## When Using Subagents

If the user asks for parallel exploration or the task is large enough to justify subagents, put this instruction at the top of each explorer prompt:

```text
A graphify knowledge graph is available at `graphify-out/GRAPH_REPORT.md`. Read that file first before doing any other exploration. Use the community clusters and node listings to navigate directly to relevant files; this replaces broad directory scanning. Read only the files the graph points you to, then use targeted searches for any remaining gaps.
```

Ask each subagent to return:

- Relevant communities consulted.
- Key files read, with line references where useful.
- Execution flow or architecture findings.
- Gaps that required targeted search outside the graph.
- The 5-10 most important files for the main thread to inspect.

## Output

Keep findings tied to concrete files. Prefer concise maps of entry points, data flow, abstraction boundaries, and test surfaces over raw graph summaries.
