const {
  Document, Packer, Paragraph, TextRun, Table, TableRow, TableCell,
  HeadingLevel, AlignmentType, BorderStyle, WidthType, ShadingType,
  VerticalAlign, PageOrientation, PageBreak
} = require('docx');
const fs = require('fs');
const path = require('path');

const C = {
  headerBg: "0A0A14", night: "0F0F1E", purple: "7B68EE", purpleLight: "EEEDFE",
  teal: "1D9E75", tealLight: "E1F5EE", coral: "D85A30", coralLight: "FAECE7",
  amber: "BA7517", amberLight: "FAEEDA", blue: "185FA5", blueLight: "E6F1FB",
  pink: "D4537E", pinkLight: "FBEAF0", green: "639922", greenLight: "EAF3DE",
  gray: "5F5E5A", grayLight: "F1EFE8", white: "FFFFFF", black: "1A1A1A",
  ph1: "E1F5EE", ph1h: "085041",
  ph2: "EEEDFE", ph2h: "3C3489",
  ph3: "FAECE7", ph3h: "712B13",
  ph4: "E6F1FB", ph4h: "0C447C",
  ph5: "FAEEDA", ph5h: "633806",
  ph6: "EAF3DE", ph6h: "27500A",
  ph7: "FBEAF0", ph7h: "72243E",
};

const border = { style: BorderStyle.SINGLE, size: 1, color: "DDDDDD" };
const borders = { top: border, bottom: border, left: border, right: border };
const heavyBorder = { style: BorderStyle.SINGLE, size: 5, color: "999999" };
const heavyBorders = { top: heavyBorder, bottom: heavyBorder, left: heavyBorder, right: heavyBorder };

function p(text, opts = {}) {
  const { bold=false, size=22, color=C.black, after=120, italic=false, align=AlignmentType.LEFT } = opts;
  return new Paragraph({ alignment: align, spacing:{ after }, children:[new TextRun({ text, bold, size, color, italic, font:"Arial" })] });
}
function h(text, level=HeadingLevel.HEADING_1) {
  return new Paragraph({ heading: level, spacing:{ before:300, after:160 }, children:[new TextRun({ text, font:"Arial" })] });
}
function rule() {
  return new Paragraph({ spacing:{ after:200 }, border:{ bottom:{ style:BorderStyle.SINGLE, size:6, color:"7B68EE", space:1 } }, children:[] });
}
function spacer(n=200) {
  return new Paragraph({ spacing:{ after:n }, children:[] });
}

const START = new Date('2026-06-01');
function weekDates(w) {
  const base = (w-1)*7;
  function fmt(d) { return d.toLocaleDateString('fr-FR',{day:'2-digit',month:'2-digit'}); }
  const mon = new Date(START); mon.setDate(mon.getDate()+base);
  const fri = new Date(START); fri.setDate(fri.getDate()+base+4);
  return `${fmt(mon)}->${fmt(fri)}`;
}

const phases = [
  { id:1, name:"Phase 1", label:"Security & Protocol",      weeks:[1,6],   bg:C.ph1, hdr:C.ph1h },
  { id:2, name:"Phase 2", label:"MVCC & Transactions",      weeks:[7,14],  bg:C.ph2, hdr:C.ph2h },
  { id:3, name:"Phase 3", label:"Performance Extreme",      weeks:[15,24], bg:C.ph3, hdr:C.ph3h },
  { id:4, name:"Phase 4", label:"Distributed Elite",        weeks:[25,36], bg:C.ph4, hdr:C.ph4h },
  { id:5, name:"Phase 5", label:"Query Engine v2",          weeks:[37,44], bg:C.ph5, hdr:C.ph5h },
  { id:6, name:"Phase 6", label:"Observabilite & Fiabilite",weeks:[45,48], bg:C.ph6, hdr:C.ph6h },
  { id:7, name:"Phase 7", label:"Ecosystem & Launch v2.0",  weeks:[49,52], bg:C.ph7, hdr:C.ph7h },
];
function getPhase(w) { return phases.find(p => w>=p.weeks[0] && w<=p.weeks[1]); }

const weeks = [
  // PHASE 1
  {w:1, lun:"Integrer rustls crate - TLS 1.3 server config",mar:"Certificats auto-signes dev avec rcgen crate",mer:"TLS sur TCP listener noedb-cli et serveur",jeu:"Client TLS - verification certificat serveur",ven:"Tests: handshake TLS reussi, faux cert rejete", del:"TLS 1.3 actif, toutes connexions chiffrees"},
  {w:2, lun:"mTLS: client presente son certificat (mutual auth)",mar:"SPIFFE-style identity dans CN du certificat",mer:"Certificate pinning pour connexions cluster Raft",jeu:"Hot reload: rotation certificats sans downtime",ven:"Tests: cert expire rejete, rotation sans coupure", del:"mTLS cluster Raft, rotation certificats live"},
  {w:3, lun:"Definir proto3 schema: QueryRequest, QueryResponse, RaftRPC",mar:"Generer code Rust avec tonic-build (gRPC)",mer:"Migrer SQL protocol sur gRPC unary calls",jeu:"Migrer AppendEntries et RequestVote sur gRPC",ven:"Tests: integration gRPC, backward compat protocol", del:"gRPC remplace TCP custom: Raft + SQL unifies"},
  {w:4, lun:"Parser PREPARE stmt AS SELECT - syntax",mar:"Statement cache: HashMap<Name, CompiledPlan>",mer:"EXECUTE stmt (val1, val2) - bind typed parameters",jeu:"Securite: parametres jamais interpoles - SQL injection impossible",ven:"Benchmark: prepared x1000 vs unprepared - speedup mesure", del:"Prepared statements, SQL injection structurellement impossible"},
  {w:5, lun:"Policy struct: table + expression + role + action",mar:"ALTER TABLE t ENABLE ROW LEVEL SECURITY - parser",mer:"CREATE POLICY p ON t USING (expr) - parser + planner",jeu:"Planner: injecter policy expr dans WHERE automatiquement",ven:"Tests: user A ne voit JAMAIS les rows de user B", del:"RLS multi-tenant: isolation par policy, zero fuite"},
  {w:6, lun:"AuditEntry: tenant, user, query, ts, rows_affected",mar:"Audit log SSTable dedie, append-only, immuable",mer:"cargo-audit: scanner toutes dependances CVEs",jeu:"cargo-fuzz: fuzzer parser SQL, 24h continus",ven:"SECURITY.md + CVE policy + post LinkedIn securite", del:"Audit log actif, 0 CVE, parser fuzz-teste 24h"},

  // PHASE 2
  {w:7,  lun:"Version struct: (key, version_ts, value, deleted_flag)",mar:"Timestamp Oracle: monotonique, distributed-safe",mer:"Modifier MemTable: multi-versions par cle (BTreeMap upgrade)",jeu:"Modifier SSTable writer/reader: format multi-version",ven:"Tests: ecrire v1 puis v2 -> lire les 2 versions par timestamp", del:"Storage multi-version: LSM MVCC-ready"},
  {w:8,  lun:"Transaction struct: txn_id, start_ts, write_set, read_set",mar:"BEGIN: creer transaction, assigner start_ts atomique",mer:"Read-your-writes: dans une txn lire ses propres writes",jeu:"COMMIT: ecrire toutes versions atomiquement avec commit_ts",ven:"ROLLBACK: purger write_set, liberer locks, marquer aborted", del:"BEGIN/COMMIT/ROLLBACK complets, isolation basique"},
  {w:9,  lun:"ReadView: snapshot des txns actives au moment du BEGIN",mar:"Visibility rule: version visible si commit_ts < start_ts",mer:"Non-repeatable reads impossibles: meme row = meme resultat",jeu:"Phantom reads: range locks sur scans pour eviter fantomes",ven:"Tests: 100 txns concurrentes -> 0 anomalie isolation mesuree", del:"Snapshot Isolation prouvee, 100 txns concurrentes sans anomalie"},
  {w:10, lun:"Lire paper SSI (Cahill 2008): Serializable Snapshot Isolation",mar:"Anti-dependency tracking: detecter write-read conflicts",mer:"Cycle detection dans dependency graph (DFS)",jeu:"Abort de la txn victime en cas de cycle detecte",ven:"Tests: bank transfers - 0 incoherence sur 10k txns paralleles", del:"SSI implemente: isolation serialisable prouvee"},
  {w:11, lun:"Wait-for graph: construire graphe dependances de lock",mar:"Cycle detection DFS: toutes les 100ms en background",mer:"Victim selection: aborter la transaction la plus jeune",jeu:"Timeout fallback: txn attendant > 5s -> abort automatique",ven:"Tests: 50 txns provoquant deadlocks -> toutes resolues, 0 hang", del:"Deadlock detection + resolution automatique, 0 hang possible"},
  {w:12, lun:"DDL transactionnel: CREATE TABLE inside BEGIN/COMMIT",mar:"Schema versioning: chaque schema porte un version_ts",mer:"Schema rollback: ROLLBACK annule le CREATE TABLE",jeu:"Online schema change: ALTER TABLE COLUMN sans table lock",ven:"Tests: 10 DDL concurrentes + 100 DML -> coherence parfaite", del:"DDL transactionnel, ALTER TABLE online sans downtime"},
  {w:13, lun:"Version pruner: identifier versions non visibles par aucune txn",mar:"Background GC: sweep SSTable, purger vieilles versions",mer:"Compaction MVCC-aware: jamais compacter version encore visible",jeu:"Metrics: avg_versions_per_key, gc_lag_seconds - Prometheus",ven:"Tests: 1M writes -> GC -> taille stable, lectures correctes", del:"GC automatique MVCC, espace disque stabilise"},
  {w:14, lun:"OLTP benchmark TPC-B style: bank transfers criterion",mar:"Mesurer: transactions/sec en mode serializable isolation",mer:"Comparer: overhead SSI vs snapshot - delta documente",jeu:"Flamegraph transaction manager -> optimiser hot path",ven:"Post LinkedIn: MVCC + Transactions + tag v1.1.0-mvcc", del:"Benchmark OLTP publie, tag v1.1.0-mvcc"},

  // PHASE 3
  {w:15, lun:"Etudier io_uring API (Axboe), integrer tokio-uring crate",mar:"Remplacer tokio::fs::File par io_uring pour WAL writes",mer:"io_uring pour SSTable reads avec registered buffers",jeu:"Batch I/O: grouper N writes en un seul io_uring submit",ven:"Benchmark: fsync io_uring vs tokio - P99 latence reduite", del:"io_uring WAL + SSTable: P99 reduit 30%+"},
  {w:16, lun:"mmap SSTable files: MAP_PRIVATE + MADV_RANDOM hints",mar:"Zero-copy: retourner &[u8] slices depuis mmap sans copie",mer:"sendfile pour transferts TCP vers client (0 kernel->user copy)",jeu:"Huge pages: MADV_HUGEPAGE sur mmap regions actives",ven:"Benchmark: mmap vs read() - throughput SELECT large +40%", del:"Zero-copy reads: throughput +40% sur SELECT large tables"},
  {w:17, lun:"std::simd nightly ou packed_simd2: setup SIMD en Rust",mar:"Vectorized filter: comparer 8 valeurs int64 en parallele AVX2",mer:"Vectorized hash: CityHash SIMD pour GROUP BY buckets",jeu:"Vectorized scan: SSTable block scan avec SIMD predicates",ven:"Benchmark: SIMD filter vs scalar 1M rows - speedup 4-8x", del:"SIMD filter AVX2: speedup 4-8x sur integer predicates"},
  {w:18, lun:"Rayon crate: parallel iterators sur les scans SSTable",mar:"Parallel SeqScan: diviser SSTable en chunks -> rayon::par_iter",mer:"Parallel hash join: build phase parallele avec rayon HashMap",jeu:"Parallel aggregate: sum/count par chunks -> merge results",ven:"Tests: resultats mono == multi-thread + benchmark speedup lineaire", del:"Parallel scan multi-core: speedup lineaire jusqu'a 8 cores"},
  {w:19, lun:"lz4-flex crate: compresser SSTable blocks a l'ecriture",mar:"Decompression transparente: hot path optimise, lazy decode",mer:"Dictionary encoding: remplacer strings repetes par IDs u16",jeu:"RLE (Run-Length Encoding): colonnes a faible cardinalite",ven:"Benchmark: taille SSTable avant/apres - cible -60% + debit maintenu", del:"LZ4 + dict encoding + RLE: taille -60%, debit inchange"},
  {w:20, lun:"Lire paper XOR filter (Graf & Lemire 2020)",mar:"Implementer XorFilter<u8> from scratch en Rust",mer:"XorFilter<u16> pour tables larges (meilleur FPR)",jeu:"Remplacer BloomFilter par XorFilter dans chaque SSTable",ven:"Tests: FPR < 0.4% (vs 1% Bloom) + taille structure -30%", del:"XOR filter: FPR 0.4%, memoire -30% vs Bloom Filter"},
  {w:21, lun:"Cache key: hash(normalized_sql + schema_version)",mar:"LRU cache en memoire: 256MB configurable via config",mer:"Invalidation: ecriture sur table T invalide tous caches de T",jeu:"Cache distribue: partager resultats via Redis cluster nodes",ven:"Benchmark: hit rate > 80% sur workload OLAP realiste", del:"Query cache LRU actif, hit rate 80%+ sur OLAP"},
  {w:22, lun:"Batch writer: accumuler N writes avant flush WAL",mar:"Group commit: plusieurs transactions en 1 seul fsync",mer:"Configurable: batch_size + batch_timeout 50ms par defaut",jeu:"Benchmark: throughput writes avec batching vs sans",ven:"Tests: durabilite garantie avec batching - crash test complet", del:"Group commit: +300% throughput writes, durabilite intacte"},
  {w:23, lun:"Runtime stats: mesurer rows_actual vs rows_estimated post-exec",mar:"Feedback loop: corriger stats si erreur estimation > 2x",mer:"Adaptive join: switcher NestedLoop -> HashJoin si sous-optimal",jeu:"Cardinality learning: histogramme online mis a jour background",ven:"Tests: optimizer converge vers plan optimal apres 3-5 executions", del:"Adaptive optimizer: plans s'ameliorent automatiquement a l'usage"},
  {w:24, lun:"YCSB workload A: 50% read / 50% update - implementer",mar:"Workloads B (95/5 read), C (100% read), F (read-modify-write)",mer:"Comparer NoeDB vs SQLite sur YCSB A, B, C - meme hardware",jeu:"Graphes results avec plotters crate, exportes en PNG",ven:"Post LinkedIn: YCSB benchmarks NoeDB v2 + tag v1.2.0-perf", del:"YCSB suite complete publiee, resultats comparatifs, tag v1.2.0"},

  // PHASE 4
  {w:25, lun:"Lire section 6 paper Raft: joint consensus algorithm",mar:"C_old,new: phase de transition deux configurations simultanees",mer:"Implementer AddVoter RPC: proposer nouveau membre",jeu:"Implementer RemoveVoter: retrait membre du cluster",ven:"Tests: cluster 3->5 noeuds sans downtime, coherence maintenue", del:"Joint consensus: membership changes 3->5->3 noeuds valides"},
  {w:26, lun:"Pre-vote phase: Candidate demande si il gagnerait sans perturber",mar:"Leader stickiness: follower rejette vote si leader recent (100ms)",mer:"Check quorum: leader verifie sa majorite sur les reads",jeu:"Tests: pre-vote empeche elections parasites en partition reseau",ven:"Mesurer: MTTR avant/apres pre-vote - cible < 200ms", del:"MTTR < 200ms, elections perturbantes eliminees"},
  {w:27, lun:"ReadIndex protocol: leader confirme sa legitimite avant read",mar:"Lease-based reads: leader utilise clock lease, pas de round-trip",mer:"Follower reads: follower attend applied_index >= read_index",jeu:"Read routing: client peut lire depuis n'importe quel noeud",ven:"Tests: linearizability verifiee - Jepsen-style checker", del:"Reads distribuees sur followers: throughput lectures x3"},
  {w:28, lun:"LeaderTransfer RPC: leader cede volontairement a un noeud cible",mar:"Forced election: target recoit TimeoutNow -> elit immediatement",mer:"Use case: rolling restart sans downtime cluster entier",jeu:"Tests: transferer leader 10x -> latence spike < 50ms chaque fois",ven:"CLI: noedb-cli raft transfer-leader --to node2", del:"Leader transfer < 50ms, rolling restart zero-downtime"},
  {w:29, lun:"2PC: Coordinator envoie PREPARE a tous les shards participants",mar:"Participant: voter YES si txn committable, persister vote sur WAL",mer:"Coordinator: si tous YES -> COMMIT global, sinon ABORT global",jeu:"Failure recovery: coordinator crash -> participants timeout -> abort",ven:"Tests: bank transfer cross-shard, 0 incoherence sur 10k txns", del:"2PC distribue: atomicite cross-shard garantie mathematiquement"},
  {w:30, lun:"ShardMap: range-based sharding (cle -> shard_id via consistent hash)",mar:"Chaque shard = groupe Raft independant avec son propre storage",mer:"Router: client envoie query -> bon shard automatiquement",jeu:"Shard split: diviser un shard surchargé en 2 avec rebalancing",ven:"Tests: 3 shards x 3 noeuds = 9 noeuds, queries routees correctement", del:"Multi-raft sharding: 3 shards operationnels, routing automatique"},
  {w:31, lun:"Distributed SeqScan: paralleliser scan sur tous les shards",mar:"Distributed aggregation: COUNT/SUM sur shards -> merge coordinator",mer:"Distributed JOIN: broadcast ou hash partition selon taille tables",jeu:"Query coordinator: plan d'execution distribue multi-shard",ven:"Benchmark: query distribuee 3 shards vs 1 shard - overhead mesure", del:"Cross-shard SELECT avec agregats: overhead < 30%"},
  {w:32, lun:"Shard metrics: rows_count, bytes, write_rate par shard",mar:"Auto-split: shard > 1GB -> trigger automatique de split",mer:"Auto-balance: migrer shard du noeud le plus charge au moins charge",jeu:"CLI: noedb-cli cluster status -> affichage heatmap par shard",ven:"Tests: 10M rows -> auto-split -> balance automatique verifie", del:"Auto-rebalancing: cluster equilibre sans intervention manuelle"},
  {w:33, lun:"Tests partition reseau: bloquer trafic inter-noeuds via namespaces",mar:"Verifier: partition 2/3 -> 1 seul leader elu dans majorite",mer:"Verifier: partition guerie -> resynchronisation < 2 secondes",jeu:"Chaos: delais reseau 0-500ms aleatoires -> cluster stable",ven:"Chaos: packet loss 5% -> toujours coherent apres 24h", del:"Rapport 48h chaos reseau: 0 split-brain, 0 corruption donnees"},
  {w:34, lun:"Lag metrics: replication_lag_bytes, replication_lag_seconds",mar:"Back-pressure: ralentir writes si follower trop en retard (> 100MB)",mer:"Configurable max_replication_lag: 100MB ou 5s par defaut",jeu:"Tests: follower lent -> back-pressure -> leader ne sature pas RAM",ven:"Alertes Prometheus: lag > seuil -> warning log + metric", del:"Back-pressure actif: OOM leader impossible, lag controle"},
  {w:35, lun:"Snapshot complet: serialiser LSM + Raft state -> archive",mar:"opendal crate: upload snapshot vers S3 / MinIO / Cloudflare R2",mer:"Restore: telecharger snapshot + replay WAL depuis object store",jeu:"Incremental backup: WAL segments uploades contiguement",ven:"Tests: backup complet -> drop toutes donnees -> restore -> data identique", del:"Backup/restore S3 fonctionnel, disaster recovery teste OK"},
  {w:36, lun:"Benchmark: throughput cluster 3 noeuds writes/s en release mode",mar:"Benchmark: throughput reads distribuees (tous followers)",mer:"Latence P99 end-to-end: client -> shard -> storage -> response",jeu:"Comparer: NoeDB cluster vs standalone Postgres sur meme hardware",ven:"Post LinkedIn: phase distributed complete + tag v1.3.0-distributed", del:"Benchmarks distribues publies, tag v1.3.0-distributed"},

  // PHASE 5
  {w:37, lun:"Parser: OVER (PARTITION BY ... ORDER BY ...) syntax complete",mar:"WindowSpec struct: partition_cols, order_cols, frame_spec",mer:"ROW_NUMBER(), RANK(), DENSE_RANK() executors implementes",jeu:"SUM() OVER, AVG() OVER, LAG(), LEAD() avec offset",ven:"Tests: 20 queries window functions -> resultats identiques Postgres", del:"Window functions: ROW_NUMBER, RANK, LAG, LEAD, SUM OVER"},
  {w:38, lun:"Parser: JSON literals, -> et ->> et #>> operators",mar:"Type JsonValue: CBOR binaire stocke dans SSTable",mer:"json_extract(), json_object(), json_array() builtins",jeu:"JSONB indexing: index B-Tree sur champ JSON data->>'user_id'",ven:"Tests: INSERT JSON -> SELECT ->> -> filter sur champ JSON", del:"JSON/JSONB first-class: read/write/index sur sous-champs"},
  {w:39, lun:"Inverted index struct: token -> list of (doc_id, positions)",mar:"Tokenizer: split, lowercase, stemming (Porter Stemmer en Rust)",mer:"CREATE INDEX fts ON articles USING FULLTEXT (body)",jeu:"MATCH(col) AGAINST('rust database') - query planning + execution",ven:"Tests: 100k articles indexes -> search latence < 5ms", del:"Full-text search: index inverse, requetes MATCH AGAINST < 5ms"},
  {w:40, lun:"Parser: WITH name AS (SELECT ...) - CTE syntax",mar:"CTE executor: materialiser CTE en table temporaire en RAM",mer:"Recursive CTE: WITH RECURSIVE - graphes et arbres hierarchiques",jeu:"Optimisation: CTE inlining quand referencee une seule fois",ven:"Tests: hierarchie employes via recursive CTE -> resultats corrects", del:"CTE + RECURSIVE operationnels, requetes hierarchiques"},
  {w:41, lun:"Parser: COPY table FROM STDIN - CSV format",mar:"Batch insert: parser CSV -> inserer 10k rows par batch Raft",mer:"Binary COPY: format binaire plus rapide que CSV",jeu:"Parallele: diviser gros fichier en chunks -> insert concurrent",ven:"Benchmark: COPY 10M rows -> throughput cible 500k rows/s", del:"COPY bulk: 500k+ rows/s, bulk loading production-grade"},
  {w:42, lun:"Integrer wasmtime crate: executer WASM modules sandboxes",mar:"CREATE FUNCTION my_func LANGUAGE wasm AS binary - SQL DDL",mer:"Sandbox: UDF ne peut ni lire storage ni acceder reseau (capabilities)",jeu:"Type bridge: Rust Row -> WASM linear memory -> resultat type",ven:"Tests: UDF compilee en WASM appelee depuis SQL - resultats corrects", del:"WASM UDFs sandboxees: extensibilite NoeDB sans recompiler"},
  {w:43, lun:"Uncorrelated subquery: SELECT (SELECT COUNT(*) FROM orders)",mar:"Correlated subquery: WHERE (SELECT SUM FROM orders WHERE uid=u.id)",mer:"Subquery decorrelation: transformer en JOIN quand possible (anti N+1)",jeu:"EXISTS / NOT EXISTS operators - pushdown vers storage",ven:"Tests: 10 correlated subqueries -> corrects + 0 N+1 query", del:"Subqueries correlees/decorrelees, EXISTS/NOT EXISTS optimises"},
  {w:44, lun:"TPC-H inspired: 8 queries analytiques complexes avec JOINs",mar:"Mesurer chaque query: P50/P95, memoire, plan choisi",mer:"Comparer vs DuckDB sur meme hardware - OLAP benchmark",jeu:"Profiling final phase 5: flamegraph -> dernieres optimisations",ven:"Tag v1.4.0-query-v2, changelog, post LinkedIn query engine", del:"TPC-H benchmark publie, tag v1.4.0-query-v2"},

  // PHASE 6
  {w:45, lun:"metrics crate: counters, histogrammes, gauges - tous modules",mar:"/metrics endpoint HTTP pour scraping Prometheus",mer:"Traces OpenTelemetry: span par query SQL + Jaeger exporter",jeu:"Grafana dashboard: latence, throughput, Raft lag, cache hits",ven:"Alertes: P99 > 100ms -> alert, leader absent > 2s -> alert PagerDuty", del:"Observabilite complete: Prometheus + OTEL + Grafana dashboard"},
  {w:46, lun:"Integrer madsim crate: simuler reseau + temps deterministiquement",mar:"Reecrire tests Raft avec madsim: inject network failures",mer:"proptest: generer sequences aleatoires d'ops sur storage + Raft",jeu:"1000 seeds x cluster 5 noeuds -> verifier 0 violation securite",ven:"Rapport: madsim coverage, bugs trouves, invariants prouves", del:"madsim 1000 seeds: 0 violation invariant Raft, bugs crush"},
  {w:47, lun:"cargo-fuzz sur parser SQL: 48h fuzzing continu LibFuzzer",mar:"cargo-fuzz sur protocol wire: frames malformees / tronquees",mer:"cargo-fuzz sur LSM storage: entrees corrompues / taille 0",jeu:"AddressSanitizer + MemorySanitizer: 0 UB ni memory error",ven:"ThreadSanitizer: 0 data race sur 10k thread interleavings", del:"48h fuzz sans crash, 0 memory error, 0 data race - certifie"},
  {w:48, lun:"Chaos monkey: tuer noeuds aleatoirement toutes les 10s pendant 1h",mar:"Mesurer: availability % sur 1h de chaos continu",mer:"RTO: mesurer temps recovery exact apres crash leader",jeu:"RPO: mesurer donnees perdues maximales apres crash en ecriture",ven:"SLA document: NoeDB garantit RTO < 500ms, RPO = 0 avec 3 noeuds", del:"SLA documente et prouve par tests: RTO<500ms, RPO=0"},

  // PHASE 7
  {w:49, lun:"Python driver: noedb-py sur PyPI (asyncio + sync via grpcio)",mar:"Go driver: noedb-go sur pkg.go.dev (gRPC + connection pool)",mer:"Node.js driver: @noe/noedb-client sur npm",jeu:"Tests cross-driver: memes queries Python/Go/Node -> resultats identiques",ven:"Docs API pour chaque driver + exemples quickstart", del:"3 drivers publies: PyPI, pkg.go.dev, npm - cross-language teste"},
  {w:50, lun:"PgBouncer-like pool en Rust: min/max connections configurables",mar:"Health check periodique des connexions dans le pool",mer:"Transaction-level pooling: connexion rendue apres COMMIT",jeu:"Load balancer: router reads -> followers, writes -> leader",ven:"Tests: 1000 connexions simultanees -> pool gere sans OOM ni panic", del:"Connection pool: 1000 connexions gerees, load balancing actif"},
  {w:51, lun:"mdBook setup: structure Architecture + Storage + Raft + Query",mar:"Storage internals: LSM, MVCC, compaction, XOR filter expliques",mer:"Distributed guide: Raft, joint consensus, 2PC, sharding expliques",jeu:"Query engine guide: planner, SIMD, adaptive optimizer expliques",ven:"Tutorial: 'Build a banking app on NoeDB in 10 minutes' - complet", del:"mdBook deploye GitHub Pages, docs completes, tutorial fonctionnel"},
  {w:52, lun:"GitHub release v2.0.0: changelog complet depuis v1.0.0",mar:"Twitter/X thread: 'I shipped NoeDB v2.0 from Cameroon'",mer:"Hacker News Show HN: NoeDB v2.0 - distributed SQL engine built in Africa",jeu:"Dev.to + Medium: '12 months building NoeDB v2 - MVCC, SIMD, Raft'",ven:"Repondre commentaires, merger PRs, celebrer - Noe Engineering est reelle", del:"LAUNCH v2.0 - target 1000+ stars, recruits inbound"},
];

// Table builders
const TW = 14400;
const cw = [440, 780, 1080, 1900, 1900, 1900, 1900, 1900, 1600];

function headerRow() {
  const hdrs = ["S#","Phase","Dates","Lundi","Mardi","Mercredi","Jeudi","Vendredi","Livrable semaine"];
  return new TableRow({ tableHeader:true, children: hdrs.map((t,i) =>
    new TableCell({
      borders, width:{ size:cw[i], type:WidthType.DXA },
      shading:{ fill:C.headerBg, type:ShadingType.CLEAR },
      verticalAlign:VerticalAlign.CENTER,
      margins:{ top:80, bottom:80, left:90, right:90 },
      children:[new Paragraph({ alignment:AlignmentType.CENTER,
        children:[new TextRun({ text:t, bold:true, size:16, color:"FFFFFF", font:"Arial" })] })]
    })
  )});
}

function phaseHdrRow(ph) {
  return new TableRow({ children:[new TableCell({
    borders: heavyBorders, columnSpan:9,
    width:{ size:TW, type:WidthType.DXA },
    shading:{ fill:ph.hdr, type:ShadingType.CLEAR },
    margins:{ top:90, bottom:90, left:160, right:100 },
    children:[new Paragraph({ children:[new TextRun({
      text:`${ph.name} -- ${ph.label}   |   Semaines ${ph.weeks[0]}-${ph.weeks[1]}`,
      bold:true, size:20, color:"FFFFFF", font:"Arial"
    })] })]
  })] });
}

function dataRow(wk) {
  const ph = getPhase(wk.w);
  const bg = wk.w % 2 === 0 ? ph.bg : C.white;
  const cols = [
    { t:String(wk.w).padStart(2,'0'), w:cw[0], bold:true, align:AlignmentType.CENTER },
    { t:ph.label, w:cw[1] },
    { t:weekDates(wk.w), w:cw[2], size:13 },
    { t:wk.lun, w:cw[3] },
    { t:wk.mar, w:cw[4] },
    { t:wk.mer, w:cw[5] },
    { t:wk.jeu, w:cw[6] },
    { t:wk.ven, w:cw[7] },
    { t:wk.del, w:cw[8], bold:true },
  ];
  return new TableRow({ children: cols.map(c =>
    new TableCell({
      borders, width:{ size:c.w, type:WidthType.DXA },
      shading:{ fill:bg, type:ShadingType.CLEAR },
      verticalAlign:VerticalAlign.TOP,
      margins:{ top:55, bottom:55, left:90, right:90 },
      children:[new Paragraph({ alignment:c.align||AlignmentType.LEFT,
        children:[new TextRun({ text:c.t, bold:c.bold||false, size:c.size||15, color:C.black, font:"Arial" })] })]
    })
  )});
}

// Upgrade summary table
const upgradeFull = [
  ["TLS/mTLS + gRPC","Wire protocol 100% chiffre, mutual auth, migration gRPC tonic","Ph.1"],
  ["Prepared statements + RLS","SQL injection impossible structurellement, isolation multi-tenant","Ph.1"],
  ["MVCC complet","Multi-version concurrency: BEGIN/COMMIT/ROLLBACK + Snapshot Isolation","Ph.2"],
  ["Serializable SSI","Serializable Snapshot Isolation (Cahill 2008), 0 anomalie","Ph.2"],
  ["Deadlock detection","Wait-for graph, victim selection, timeout - 0 hang possible","Ph.2"],
  ["io_uring + zero-copy","P99 I/O reduit 30%+, sendfile vers client, MADV_HUGEPAGE","Ph.3"],
  ["SIMD vectorized exec","AVX2 filter 8 int64 en parallele - speedup 4-8x sur predicates","Ph.3"],
  ["Rayon parallel queries","Scans multi-core lineaires, hash join parallele","Ph.3"],
  ["LZ4 + XOR filter","SSTable -60% taille, FPR 0.4% (meilleur que Bloom)","Ph.3"],
  ["Group commit + query cache","Throughput writes +300%, cache LRU 80%+ hit rate OLAP","Ph.3"],
  ["Joint consensus","Dynamic membership: AddVoter/RemoveVoter sans downtime","Ph.4"],
  ["Follower reads linearisables","Throughput reads x3, tous noeuds servent les lectures","Ph.4"],
  ["Distributed 2PC","Transactions atomiques cross-shard, 0 incoherence","Ph.4"],
  ["Multi-raft sharding","Range sharding, auto-split > 1GB, auto-rebalancing","Ph.4"],
  ["S3 backup + restore","Disaster recovery teste, backup incremental, RTO<500ms RPO=0","Ph.4"],
  ["Window functions","ROW_NUMBER, RANK, LAG, LEAD, SUM OVER - analytics complets","Ph.5"],
  ["JSON/JSONB + Full-text search","JSON first-class, index sur sous-champs, MATCH AGAINST < 5ms","Ph.5"],
  ["WASM UDFs","Fonctions custom sandboxees WASM, extensible sans recompiler","Ph.5"],
  ["COPY bulk + CTE recursive","500k rows/s ingestion, WITH RECURSIVE pour graphes","Ph.5"],
  ["Prometheus + OpenTelemetry","Observabilite complete: metrics, traces, Grafana dashboard","Ph.6"],
  ["madsim + fuzz + sanitizers","1000 seeds chaos tests, 48h fuzz, 0 memory error, 0 data race","Ph.6"],
  ["Python / Go / Node.js drivers","3 drivers publies, connection pool 1000+ connexions","Ph.7"],
  ["mdBook documentation","Architecture, storage, Raft, query engine - docs completes","Ph.7"],
];

function upgradeFinalTable() {
  const TW2 = 9360;
  const cw2 = [2600, 5360, 1400];
  const bgMap = {"Ph.1":C.ph1,"Ph.2":C.ph2,"Ph.3":C.ph3,"Ph.4":C.ph4,"Ph.5":C.ph5,"Ph.6":C.ph6,"Ph.7":C.ph7};
  return new Table({
    width:{ size:TW2, type:WidthType.DXA }, columnWidths:cw2,
    rows:[
      new TableRow({ tableHeader:true, children:[
        ["Module ajoute","Ce que ca apporte","Phase"].map((t,i) => new TableCell({
          borders, width:{ size:cw2[i], type:WidthType.DXA },
          shading:{ fill:C.headerBg, type:ShadingType.CLEAR },
          margins:{ top:80, bottom:80, left:120, right:120 },
          children:[new Paragraph({ children:[new TextRun({ text:t, bold:true, size:17, color:"FFFFFF", font:"Arial" })] })]
        }))
      ]}),
      ...upgradeFull.map(([mod,desc,ph],i) => new TableRow({ children:[
        new TableCell({ borders, width:{ size:cw2[0], type:WidthType.DXA },
          shading:{ fill:i%2===0?bgMap[ph]:C.white, type:ShadingType.CLEAR },
          margins:{ top:60, bottom:60, left:120, right:120 },
          children:[new Paragraph({ children:[new TextRun({ text:mod, bold:true, size:15, font:"Arial", color:C.black })] })] }),
        new TableCell({ borders, width:{ size:cw2[1], type:WidthType.DXA },
          shading:{ fill:i%2===0?bgMap[ph]:C.white, type:ShadingType.CLEAR },
          margins:{ top:60, bottom:60, left:120, right:120 },
          children:[new Paragraph({ children:[new TextRun({ text:desc, size:15, font:"Arial", color:C.black })] })] }),
        new TableCell({ borders, width:{ size:cw2[2], type:WidthType.DXA },
          shading:{ fill:i%2===0?bgMap[ph]:C.white, type:ShadingType.CLEAR },
          margins:{ top:60, bottom:60, left:120, right:120 },
          children:[new Paragraph({ alignment:AlignmentType.CENTER, children:[new TextRun({ text:ph, bold:true, size:15, font:"Arial", color:C.black })] })] }),
      ]}))
    ]
  });
}

// Sprint table
const sprintRows = [headerRow()];
let curPh = null;
for (const wk of weeks) {
  const ph = getPhase(wk.w);
  if (!curPh || curPh.id !== ph.id) { sprintRows.push(phaseHdrRow(ph)); curPh = ph; }
  sprintRows.push(dataRow(wk));
}
const sprintTable = new Table({ width:{ size:TW, type:WidthType.DXA }, columnWidths:cw, rows:sprintRows });

// Cover page
const cover = [
  spacer(2000),
  new Paragraph({ alignment:AlignmentType.CENTER, spacing:{ after:200 }, children:[new TextRun({ text:"NoeDB", bold:true, size:80, color:C.purple, font:"Arial" })] }),
  new Paragraph({ alignment:AlignmentType.CENTER, spacing:{ after:160 }, children:[new TextRun({ text:"v2.0 -- Sprint Plan", bold:true, size:36, color:C.teal, font:"Arial" })] }),
  new Paragraph({ alignment:AlignmentType.CENTER, spacing:{ after:200 }, children:[new TextRun({ text:"A Distributed Embedded SQL Engine in Rust", size:26, color:C.gray, italic:true, font:"Arial" })] }),
  new Paragraph({ alignment:AlignmentType.CENTER, spacing:{ after:80 }, border:{ bottom:{ style:BorderStyle.SINGLE, size:4, color:C.purple, space:1 } }, children:[] }),
  spacer(200),
  new Paragraph({ alignment:AlignmentType.CENTER, spacing:{ after:120 }, children:[new TextRun({ text:"Noe Engineering  |  Douala, Cameroun", size:22, color:C.gray, font:"Arial" })] }),
  new Paragraph({ alignment:AlignmentType.CENTER, spacing:{ after:120 }, children:[new TextRun({ text:"Debut : 1 juin 2026  |  Fin : 29 mai 2027", size:22, color:C.gray, font:"Arial" })] }),
  new Paragraph({ alignment:AlignmentType.CENTER, spacing:{ after:2000 }, children:[new TextRun({ text:"7 phases  |  52 semaines  |  23 modules nouveaux  |  Rust", size:22, color:C.gray, font:"Arial" })] }),
  new Paragraph({ children:[new PageBreak()] }),
];

const intro = [
  h("Ce que NoeDB v2.0 ajoute sur v1.0", HeadingLevel.HEADING_1),
  rule(),
  p("NoeDB v1.0 a pose les fondations : Lexer, Parser, Storage Engine (LSM), Query Planner, Consensus Raft, Engine E2E, Protocol et CLI. NoeDB v2.0 transforme ce moteur en infrastructure production-grade, enterprise-ready, et world-class.", { after:180 }),
  p("Ce sprint de 52 semaines ajoute 23 modules critiques repartis en 7 phases. Chaque phase livre des capacites mesurables, testees, et benchmarkees.", { after:240 }),
  upgradeFinalTable(),
  spacer(200),
  new Paragraph({ children:[new PageBreak()] }),
  h("Sprint Planning -- 52 semaines jour par jour", HeadingLevel.HEADING_1),
  rule(),
  p("Debut : 1er juin 2026  |  Fin : 29 mai 2027  |  7 phases  |  52 semaines", { bold:true, after:300 }),
];

const doc = new Document({
  styles: {
    default: { document: { run: { font:"Arial", size:22 } } },
    paragraphStyles: [
      { id:"Heading1", name:"Heading 1", basedOn:"Normal", next:"Normal", quickFormat:true,
        run:{ size:38, bold:true, font:"Arial", color:C.purple },
        paragraph:{ spacing:{ before:400, after:200 }, outlineLevel:0 } },
      { id:"Heading2", name:"Heading 2", basedOn:"Normal", next:"Normal", quickFormat:true,
        run:{ size:28, bold:true, font:"Arial", color:C.gray },
        paragraph:{ spacing:{ before:280, after:140 }, outlineLevel:1 } },
    ]
  },
  sections: [
    {
      properties:{ page:{ size:{ width:12240, height:15840 }, margin:{ top:1440, right:1440, bottom:1440, left:1440 } } },
      children:[ ...cover, ...intro ]
    },
    {
      properties:{ page:{ size:{ width:12240, height:15840, orientation:PageOrientation.LANDSCAPE }, margin:{ top:700, right:700, bottom:700, left:700 } } },
      children:[ sprintTable ]
    }
  ]
});

const DOCS = path.join(__dirname, '..', 'docs');
const OUT_DOCX = path.join(DOCS, 'NoeDB_v2_Sprint_Plan.docx');
const OUT_HTML = path.join(DOCS, 'NoeDB_v2_Sprint_Plan.html');

function esc(s) {
  return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

function writeHtml() {
  const phaseColors = {
    1: ['#E1F5EE','#085041'], 2: ['#EEEDFE','#3C3489'], 3: ['#FAECE7','#712B13'],
    4: ['#E6F1FB','#0C447C'], 5: ['#FAEEDA','#633806'], 6: ['#EAF3DE','#27500A'], 7: ['#FBEAF0','#72243E'],
  };
  let rows = '';
  let cur = 0;
  for (const wk of weeks) {
    const ph = getPhase(wk.w);
    if (ph.id !== cur) {
      cur = ph.id;
      const [bg, fg] = phaseColors[ph.id];
      rows += `<tr class="phase"><td colspan="9" style="background:${fg};color:#fff;font-weight:bold;padding:10px">Phase ${ph.id} — ${esc(ph.label)} (S${ph.weeks[0]}–${ph.weeks[1]})</td></tr>\n`;
    }
    rows += `<tr>
      <td>${String(wk.w).padStart(2,'0')}</td>
      <td>${esc(ph.label)}</td>
      <td>${weekDates(wk.w)}</td>
      <td>${esc(wk.lun)}</td><td>${esc(wk.mar)}</td><td>${esc(wk.mer)}</td>
      <td>${esc(wk.jeu)}</td><td>${esc(wk.ven)}</td>
      <td><strong>${esc(wk.del)}</strong></td></tr>\n`;
  }
  const upgradeRows = upgradeFull.map(([m,d,ph]) =>
    `<tr><td><strong>${esc(m)}</strong></td><td>${esc(d)}</td><td>${ph}</td></tr>`).join('\n');

  const html = `<!DOCTYPE html>
<html lang="fr"><head>
<meta charset="utf-8"/>
<title>NoeDB v2.0 — Sprint Plan</title>
<style>
  @page { size: A4 landscape; margin: 12mm; }
  body { font-family: Arial, sans-serif; font-size: 11px; color: #1a1a1a; margin: 24px; }
  h1 { color: #7B68EE; font-size: 28px; margin-bottom: 4px; }
  h2 { color: #1D9E75; font-size: 18px; }
  .meta { color: #5F5E5A; margin-bottom: 24px; }
  table { border-collapse: collapse; width: 100%; margin: 16px 0; }
  th { background: #0A0A14; color: #fff; padding: 8px 6px; font-size: 10px; text-align: left; }
  td { border: 1px solid #ddd; padding: 6px; vertical-align: top; font-size: 9px; }
  tr:nth-child(even):not(.phase) { background: #fafafa; }
  tr.phase td { font-size: 12px; }
  .cover { text-align: center; padding: 80px 0 40px; page-break-after: always; }
  .cover h1 { font-size: 48px; }
  @media print { .no-print { display: none; } body { margin: 0; } }
</style></head><body>
<div class="cover">
  <h1>NoeDB</h1>
  <h2>v2.0 — Sprint Plan</h2>
  <p class="meta">Noe Engineering · Douala, Cameroun<br/>
  Début 1 juin 2026 · Fin 29 mai 2027 · 7 phases · 52 semaines · 23 modules</p>
</div>
<p class="no-print"><strong>PDF :</strong> Fichier → Imprimer → Enregistrer en PDF (ou ouvrir le .docx dans LibreOffice → Exporter PDF).</p>
<h2>Ce que v2.0 ajoute sur v1.0</h2>
<table><thead><tr><th>Module</th><th>Apport</th><th>Phase</th></tr></thead><tbody>${upgradeRows}</tbody></table>
<h2>Sprint — 52 semaines</h2>
<table>
<thead><tr><th>S#</th><th>Phase</th><th>Dates</th><th>Lundi</th><th>Mardi</th><th>Mercredi</th><th>Jeudi</th><th>Vendredi</th><th>Livrable</th></tr></thead>
<tbody>${rows}</tbody></table>
</body></html>`;
  fs.writeFileSync(OUT_HTML, html);
  console.log('HTML:', OUT_HTML);
}

Packer.toBuffer(doc).then(buf => {
  fs.mkdirSync(DOCS, { recursive: true });
  fs.writeFileSync(OUT_DOCX, buf);
  writeHtml();
  console.log('DOCX:', OUT_DOCX);
  console.log('Size:', buf.length, 'bytes');
  console.log('Weeks:', weeks.length);
}).catch(err => { console.error(err); process.exit(1); });
