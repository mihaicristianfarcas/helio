# Contributing

Thank you for helping. Helio is a small project with a high bar for the
data path — it is a real-time system, and most of its decisions were
measured — so the best contributions come with a reason and a way to check
them.

## Reporting a problem

[Open an issue](https://github.com/mihaicristianfarcas/helio/issues/new/choose)
and choose a form:

- **Bug report**: what happened, on which systems, with the logs from both
  sides. [docs/logs.md](docs/logs.md) says where they are, and how to turn
  on debug logging for anything about smoothness or latency.
- **Tester report**: you tried Helio and want to say how it went. It takes
  under two minutes, and "it worked" is as useful as "it didn't".

Questions go to
[Discussions](https://github.com/mihaicristianfarcas/helio/discussions/categories/q-a).
Security problems go privately instead: see [SECURITY.md](SECURITY.md).

## Proposing a change

For anything larger than a fix, open an issue first and say what you want
to change and why: a design that fits in the first try saves both of us
time. Moonlight and Sunshine/Apollo are the reference for streaming
behaviour, so "Moonlight does it this way" is a good argument, and a
measurement is the best one.

## Making the change

1. Set up a build: [docs/install.md](docs/install.md#building-from-source)
   and [docs/development.md](docs/development.md), and the repository's git
   hooks: `git config core.hooksPath .githooks`.
2. Follow [AGENTS.md](AGENTS.md): the rules for code, comments, docs and
   commits apply to everyone, not only to AI agents.
3. Update the documentation in the same change:
   [AGENTS.md](AGENTS.md#what-goes-where) says which page each kind of
   change touches, and [how it is written](AGENTS.md#how-it-is-written).
4. Check it on every platform you can, and type-check the rest
   ([AGENTS.md, "Before you finish"](AGENTS.md#before-you-finish)).
5. Open a pull request: its template asks what changed, why, how you
   verified it, and what you could not verify, with evidence a reviewer can
   see. `main` takes changes only through pull requests, once CI passes
   (formatting, clippy and the tests on all three systems).

Using an AI coding agent is fine; you are responsible for what it wrote, as
if you had written it.

Contributions are accepted under the project's license, the
[GPL-3.0](LICENSE). Do not include code copied or translated from
Moonlight, Sunshine or Apollo: Helio follows their behaviour, not their
source. Code from elsewhere comes only under a license compatible with the
GPL-3.0, with its notice.
