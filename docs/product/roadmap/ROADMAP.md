# gitserious Public Roadmap

gitserious is pre-1.0.

| Release | Goal | Status |
| --- | --- | --- |
| v0.1.0 | Project Configuration and Structured Git Commit Authoring | in-progress |
| v0.2.0 | MCP (v2) and Integrations | planned |
| v0.3.0 | Inferential Engineering Intelligence | planned |

Only the in-progress release is a commitment. Planned releases describe where
we're headed, and their scope will change as v0.1 gets real use.

---

### v0.1.0 - Project Configuration and Structured Git Commit Authoring

Establish the foundation for capturing structured, evidence-backed software
changes.

- **Project Configuration:** Define and manage project-level configuration,
  including commit conventions and taxonomies.
- **Structured Commit Authoring:** Author consistent, structured commit messages
  with explicit change rationale and supporting evidence.
- **Taxonomy Management:** Create, customize, and manage commit taxonomies
  through project and global configuration.
- **Commit Integrity:** Ensure authored commits accurately reflect the changes
  being recorded.

---

### v0.2.0 - MCP (v2) and Integrations

Make gitserious accessible to external tools, development workflows, and AI
agents.

- **MCP Server:** Expose gitserious capabilities through a standardized Model
  Context Protocol interface.
- **GitHub Integration:** Connect commit history with pull requests, reviews,
  and other relevant development activity.
- **Linear Integration:** Connect software changes with issues, projects, and
  engineering work items.
- **External Context:** Make relevant development context accessible across
  integrated tools and workflows.

---

### v0.3.0 - Inferential Engineering Intelligence

Turn accumulated software-change history and supporting evidence into
actionable engineering knowledge.

- **Historical Intelligence:** Index, retrieve, and query historical changes
  using their rationale, evidence, and associated development activity.
- **Symbol Analysis:** Parse source code to identify symbols, properties,
  dependencies, and relationships between them. Associate structural changes
  with their historical context.
- **Blast Radius Analysis:** Trace dependencies and historical change
  relationships to identify potentially affected symbols, components, and
  code paths.
- **Root Cause Analysis:** Correlate regressions and engineering problems with
  historical changes, their rationale, and supporting evidence to identify
  candidate causes.
- **Change Pattern Analysis:** Identify recurring patterns across software
  changes, including co-changing symbols, recurring modifications, and
  relationships between seemingly unrelated changes.
- **Engineering Insights:** Surface relevant historical context, structural
  relationships, and inferred findings to inform implementation and debugging
  decisions.

---

_Last updated: 2026-11-22_
