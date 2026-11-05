<!--
Start with one or two short paragraphs, 120 words at most, written the way
you would explain the change to a colleague at their desk: what a person saw,
and what they see now. Name the symptom, not a category. No headers, bullets
or file names here: internal names go under How.

A reviewer should understand the change and see that it works before reading
a line of How. Delete any section below that adds nothing.
-->

## Before and after

<!--
Evidence made for this pull request, from the same input on main and on this
branch. Give each figure a bold title of two to five words, then one sentence:
what input was replayed, what differs, and where to look. Every figure answers
a question a reviewer would ask; if it does not, cut it.

  **Hovering a host.** Same scripted pointer on both builds: on main the card
  jumps a pixel, now it stays.

  | main | this branch |
  |---|---|
  | ![main: the card jumps](./main.png) | ![this branch: it stays](./branch.png) |

Pairs go in a table, wide subjects in one stacked image, motion in one GIF with
both builds in it. Screenshots come from HELIO_UI_DEMO (docs/ui.md), with sample
data only. Attach media (`gh pr create --attach ./main.png`, or paste it); do
not commit it. A change with nothing to look at shows a table of what changed
(before, after, source), or a measurement with its conditions.
-->

## Edge cases

<!-- The awkward inputs, and every existing flow that goes through what you changed. -->

| Case | How checked | Result |
|---|---|---|

---

## How

<!-- The decisions that are not obvious, and what you tried and dropped, with why. -->

## Verification

<!--
Results, not claims: "519 passed, 0 failed, 2 ignored", not "tests pass". Say
which platforms, and how: CI, real hardware, a virtual machine, a container, or a
type-check only. Moonlight or Sunshine behaviour: the source file. A measurement:
the machine, the settings and the numbers. Docs updated, as AGENTS.md's
"What goes where" table asks.
-->

## Not in this pull request

<!--
The scope you drew, what you could not verify (a platform you could not run,
hardware you do not have), and anything you noticed and left. Text written ahead
of a release: what has to exist before this merges.

When they apply, add: Review first (the two to four spots you are least sure of),
Merge notes (conflicts with open pull requests), Tests changed, and why.
-->
