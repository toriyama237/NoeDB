# NoeDB v2.0 — Sprint planning

Fichiers générés à partir de `scripts/gen_noedb_v2_sprint.js` :

| Fichier | Usage |
|---------|--------|
| `NoeDB_v2_Sprint_Plan.docx` | Planning détaillé (Word / LibreOffice), table paysage 52 semaines |
| `NoeDB_v2_Sprint_Plan.html` | Même contenu — **imprimer en PDF** depuis le navigateur |

## Régénérer

```bash
cd ~/NoeDB/scripts
npm install    # une fois (dépendance docx)
npm run sprint-plan
```

## Obtenir le PDF sur ton ThinkPad

**Option A — HTML (recommandé)**  
```bash
xdg-open ~/NoeDB/docs/NoeDB_v2_Sprint_Plan.html
# Ctrl+P → Enregistrer au format PDF
```

**Option B — DOCX**  
```bash
libreoffice ~/NoeDB/docs/NoeDB_v2_Sprint_Plan.docx
# Fichier → Exporter au format PDF → NoeDB_v2_Sprint_Plan.pdf
```

**Option C — ligne de commande**  
```bash
cd ~/NoeDB/docs
libreoffice --headless --convert-to pdf NoeDB_v2_Sprint_Plan.docx
```

## Calendrier

- **Début :** 1 juin 2026  
- **Fin :** 29 mai 2027  
- **Release cible :** `v2.0.0` (semaine 52)

## Phases

1. Security & Protocol (S1–6)  
2. MVCC & Transactions (S7–14)  
3. Performance Extreme (S15–24)  
4. Distributed Elite (S25–36)  
5. Query Engine v2 (S37–44)  
6. Observabilité & Fiabilité (S45–48)  
7. Ecosystem & Launch (S49–52)
