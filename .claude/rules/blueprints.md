# Radiance blueprints

This repo follows the shared Radiance blueprints: https://github.com/Radiance-Philippines/blueprints
(locally `~/Github/blueprints`; this file is a committed copy of `claude/templates/repo-rules-blueprints.md`).
Read the blueprint that matches the work before changing it; it wins over this summary.

| Work | Blueprint | Short rule |
|---|---|---|
| Motion, transitions, press states | `ANIMATION_BLUEPRINT.md` | Motion only (screenshot at rest unchanged), tokens not inline values, reduced motion in the same change; PR table §7 |
| Web pages: images, caching, fonts, bundles, scripts | `PERFORMANCE_BLUEPRINT.md` | Budgets §1 (JS ≤ 130 KB gzip first visit), `Picture` lazy by default, cache headers §3, lazy pages; PR table §9 |
| Second language, copy, language codes | `FILIPINO_BLUEPRINT.md` | It is Filipino (`fil`/`fil-PH`), never Tagalog/Taglish; read old `tl`, write `fil`; the owner's glossary §7; PR table §11 |
| Input fields on any platform | `FORMS_BLUEPRINT.md` | Every field has its autocomplete/keyboard hint; someone else's data gets `off`; passwords always `current-`/`new-password`; PR list §5 |
| Third-party UI skills | `DESIGN_SKILLS.md` | Which design skills to use and which house rules override them |

Builds and tests run on the ribs02 build server, never on a laptop.
On the owner's laptop the same rules also load from `~/.claude/rules/` (path-scoped short forms of each blueprint).
