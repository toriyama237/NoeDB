# Git workflow — NoeDB (audit-ready)

Objectif : un historique lisible par un auditeur externe (banque, ANSSI, due diligence).

## Branches

| Préfixe | Usage | Exemple |
|---------|--------|---------|
| `main` | Releases stables, toujours testable (`cargo test --workspace`) | — |
| `develop` | Intégration des features validées en local | — |
| `feat/` | Fonctionnalité lourde ou exemple de bout en bout | `feat/national-hr-audit-example` |
| `fix/` | Correction de bug traçable à un audit / ticket | `fix/planner-hash-join-keys` |
| `perf/` | Optimisation mesurable (bench avant/après) | `perf/subquery-execution` |
| `refactor/` | Restructuration sans changement de comportement | `refactor/planner-build` |

**Règle** : une branche = un sujet. Pas de mélange fix + feat + refactor.

## Commits

Format [Conventional Commits](https://www.conventionalcommits.org/) :

```
type(scope): description impérative courte

Corps optionnel : pourquoi (audit), pas seulement quoi.
Réf. audit : N16_cte, phase5_cte, insert atomicity, etc.
```

Types : `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `chore`.

**Règles audit** :
- commits atomiques (compilent + tests ciblés passent si possible) ;
- pas de « WIP » sur `main` / `develop` ;
- pas de `git commit --amend` après push ;
- merge `--no-ff` pour garder la frontière de branche dans l’historique.

## Flux local

```bash
git checkout develop
git pull origin develop          # si remote à jour
git checkout -b fix/mon-sujet

# … dev + tests …
cargo test --workspace

git add …
git commit -m "fix(planner): …"

git checkout develop
git merge --no-ff fix/mon-sujet -m "merge: fix/planner-…"
cargo test --workspace

git checkout main
git merge --no-ff develop -m "release: …"   # quand prêt pour release
```

## Remotes

- `origin` — GitHub (CI, PRs)
- `gitlab` — GitLab / OVH (miroir ou CI secondaire)

Pousser chaque branche feature avant merge long :

```bash
git push -u origin fix/mon-sujet
```

## Checklist avant merge vers `develop`

- [ ] `cargo test --workspace` vert
- [ ] `cargo clippy --workspace -- -D warnings` (si CI billing OK)
- [ ] message de commit cite le test ou l’audit qui motivait le changement
- [ ] pas de secrets, `.env`, clés dans le diff
