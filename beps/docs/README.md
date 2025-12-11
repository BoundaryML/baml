# BEP — BAML Enhancement Proposals

BEPs are design proposals for evolving the BAML language and core tooling.

Below is an auto-generated index of all BEPs.

> ⚠️ Do not edit the table below by hand.
> Run the BEP update script instead (see instructions at the bottom).

<!-- BEP-TABLE-START -->

| Status | Meaning |
| :--- | :--- |
| <img src="https://img.shields.io/badge/Status-Draft-lightgrey" alt="Draft"> | Work in progress, not ready for review |
| <img src="https://img.shields.io/badge/Status-Proposed-yellow" alt="Proposed"> | Ready for review and discussion |
| <img src="https://img.shields.io/badge/Status-Accepted-brightgreen" alt="Accepted"> | Approved for implementation |
| <img src="https://img.shields.io/badge/Status-Implemented-blue" alt="Implemented"> | Feature is live in BAML |
| <img src="https://img.shields.io/badge/Status-Rejected-red" alt="Rejected"> | Decided against |
| <img src="https://img.shields.io/badge/Status-Superseded-orange" alt="Superseded"> | Replaced by another BEP |

<table>
  <thead>
    <tr>
      <th>BEP</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><a href="./proposals/BEP-001-exceptions/"><strong>BEP-001</strong>: Exception Handling</a> &nbsp; <img src="https://img.shields.io/badge/Status-Proposed-yellow" alt="Proposed"><br><br><br><br><span style='font-size:0.8em; color:gray'>Shepherd(s): Vaibhav Gupta <vbv@boundaryml.com></span></td>
    </tr>
    <tr>
      <td><a href="./proposals/BEP-002-match/"><strong>BEP-002</strong>: match</a> &nbsp; <img src="https://img.shields.io/badge/Status-Accepted-brightgreen" alt="Accepted"><br><br><br><br><span style='font-size:0.8em; color:gray'>Shepherd(s): hellovai <vbv@boundaryml.com></span></td>
    </tr>
    <tr>
      <td><a href="./proposals/BEP-005-prompt-optimization/"><strong>BEP-005</strong>: Prompt Optimization</a> &nbsp; <img src="https://img.shields.io/badge/Status-Accepted-brightgreen" alt="Accepted"><br><br>- `baml-cli` supports a new command called `optimize` that writes and improves prompts for you, similar to [DSPy](https://dspy.ai/). - Prompt optimization attempts to maximize the number of passing BAML test cases, and optionally minimize tokens and latency. - The optimizer is based on the [GEPA algorithm](https://arxiv.org/abs/2507.19457), which is partially encoded as BAML functions that you can tweak.<br><br><span style='font-size:0.8em; color:gray'>Shepherd(s): Greg Hale <imalsogreg@gmail.com></span></td>
    </tr>
    <tr>
      <td><a href="./proposals/BEP-006-compiler-errors/"><strong>BEP-006</strong>: compiler-errors</a> &nbsp; <img src="https://img.shields.io/badge/Status-Draft-lightgrey" alt="Draft"><br><br>Uniform user-facing experience for compiler errors.<br><br><span style='font-size:0.8em; color:gray'>Shepherd(s): Greg Hale <greg@boundary.com></span></td>
    </tr>
  </tbody>
</table>

<!-- BEP-TABLE-END -->

---

## Management

> Scripts are self-contained Python scripts using [`uv`](https://github.com/astral-sh/uv). Ensure `uv` is installed.

### Creating a new BEP

To create a new proposal:
```bash
mise run bep:new -- "Feature Name"
```

This will:
1. Create a new directory `beps/BEP-XXX-feature-name/`
2. Create a `README.md` template inside it with the next available BEP ID.

### Updating the Index

After modifying any BEP, update this README table:
```bash
mise run bep:readme
```

### Managing BEPs

To update a BEP's status or timestamp:

**Touch (Update Timestamp):**
```bash
mise run bep:update 001
# OR
mise run bep:update BEP-001-exceptions
```

**Change Status:**
```bash
mise run bep:update 001 --status Proposed
```
(Valid statuses: Draft, Proposed, Accepted, Implemented, Rejected, Superseded)

