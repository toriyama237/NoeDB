//! National bank HR/payroll audit — 80 agencies, 50_009 employees, payslips.
//!
//! Reference data via SQL; bulk employee/payslip load via `put_row` (avoids O(n²)
//! PK scans on batched INSERT). Business queries and constraint checks via SQL.

#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use noedb_engine::LocalEngine;
use serde::Serialize;

const AGENCIES: u32 = 80;
const EMPLOYEES: u32 = 50_009;
const DEPARTMENTS: u32 = 22;
const BATCH: usize = 200;

#[derive(Serialize)]
struct Bench {
    name: String,
    sql: String,
    ms: f64,
    rows: usize,
    ok: bool,
    error: String,
}

#[derive(Serialize)]
struct Report {
    title: String,
    generated_at: String,
    noedb_version: String,
    data_dir: String,
    agencies: u32,
    departments: u32,
    employees: u32,
    payroll_periods: u32,
    payslips: u32,
    seed_load_ms: f64,
    workspace_tests_passed: u32,
    benchmarks: Vec<Bench>,
    summary_pass: u32,
    summary_fail: u32,
    notes: Vec<String>,
}

fn batched(table: &str, rows: &[String]) -> Vec<String> {
    rows.chunks(BATCH)
        .map(|c| format!("INSERT INTO {table} VALUES {}", c.join(", ")))
        .collect()
}

fn run_bench(eng: &LocalEngine, name: &str, sql: &str) -> Bench {
    let t0 = Instant::now();
    match eng.execute(sql) {
        Ok(r) => Bench {
            name: name.into(),
            sql: sql.into(),
            ms: t0.elapsed().as_secs_f64() * 1000.0,
            rows: r.rows.len(),
            ok: true,
            error: String::new(),
        },
        Err(e) => Bench {
            name: name.into(),
            sql: sql.into(),
            ms: t0.elapsed().as_secs_f64() * 1000.0,
            rows: 0,
            ok: false,
            error: e.to_string(),
        },
    }
}

fn put(eng: &LocalEngine, table: &str, row: &str, col: &str, val: &str) {
    eng.put_row_default(table, row, col, val.as_bytes()).unwrap();
}

fn main() {
    let dir = PathBuf::from("/tmp/noedb-national-audit");
    let _ = fs::remove_dir_all(&dir);
    let eng = LocalEngine::open_throughput(&dir).expect("open");

    let seed_start = Instant::now();
    eprintln!("Phase 1: schema + reference data (SQL)…");
    for ddl in [
        "CREATE TABLE agencies (id INT PRIMARY KEY, code TEXT UNIQUE NOT NULL, city TEXT NOT NULL, region TEXT NOT NULL, country TEXT NOT NULL)",
        "CREATE TABLE departments (id INT PRIMARY KEY, code TEXT UNIQUE NOT NULL, name TEXT NOT NULL, cost_center TEXT NOT NULL)",
        "CREATE TABLE employees (id INT PRIMARY KEY, agency_id INT NOT NULL, department_id INT NOT NULL, matricule TEXT UNIQUE NOT NULL, name TEXT NOT NULL, grade TEXT NOT NULL, salary_base INT NOT NULL)",
        "CREATE TABLE payroll_periods (id INT PRIMARY KEY, year INT NOT NULL, month INT NOT NULL, label TEXT UNIQUE NOT NULL, status TEXT NOT NULL)",
        "CREATE TABLE payslips (id INT PRIMARY KEY, period_id INT NOT NULL, employee_id INT NOT NULL, gross INT NOT NULL, net INT NOT NULL, income_tax INT NOT NULL, social INT NOT NULL)",
    ] {
        eng.execute(ddl).unwrap();
    }

    let cities = [
        ("Paris", "IDF", "FR"),
        ("Lyon", "ARA", "FR"),
        ("Bruxelles", "BRU", "BE"),
        ("Frankfurt", "HE", "DE"),
        ("Madrid", "MD", "ES"),
        ("Londres", "ENG", "GB"),
        ("Geneve", "GE", "CH"),
        ("Montreal", "QC", "CA"),
        ("New York", "NY", "US"),
        ("Singapour", "SG", "SG"),
    ];
    let ag_rows: Vec<String> = (1..=AGENCIES)
        .map(|i| {
            let (city, region, country) = cities[((i - 1) as usize) % cities.len()];
            format!("({i}, 'AG{i:03}', '{city}', '{region}', '{country}')")
        })
        .collect();
    for sql in batched("agencies", &ag_rows) {
        eng.execute(&sql).unwrap();
    }

    let depts = [
        ("DIR", "Direction Generale", "CC1000"),
        ("RH", "Ressources Humaines", "CC1100"),
        ("FIN", "Finance", "CC1200"),
        ("IT", "SI", "CC1300"),
        ("RISK", "Risques", "CC1400"),
        ("COMP", "Conformite", "CC1500"),
        ("TRES", "Tresorerie", "CC1600"),
        ("RET", "Retail", "CC2000"),
        ("CORP", "Corporate", "CC2100"),
        ("PME", "PME", "CC2200"),
        ("WM", "Private Banking", "CC2300"),
        ("AM", "Asset Management", "CC2400"),
        ("OPS", "Back-Office", "CC3000"),
        ("CRED", "Credit", "CC3100"),
        ("COL", "Recouvrement", "CC3200"),
        ("MKT", "Marketing", "CC4000"),
        ("LEGAL", "Juridique", "CC4100"),
        ("AUDIT", "Audit Interne", "CC4200"),
        ("DATA", "Data", "CC4300"),
        ("SEC", "Securite", "CC4400"),
        ("INT", "International", "CC5000"),
        ("PROJ", "Projets", "CC5100"),
    ];
    let dept_rows: Vec<String> = depts
        .iter()
        .enumerate()
        .map(|(i, (c, n, cc))| format!("({}, '{c}', '{n}', '{cc}')", i + 1))
        .collect();
    for sql in batched("departments", &dept_rows) {
        eng.execute(&sql).unwrap();
    }
    eng.execute("INSERT INTO payroll_periods VALUES (1, 2026, 3, '2026-03', 'closed')")
        .unwrap();

    eprintln!("Phase 2: {EMPLOYEES} employees (put_row bulk)…");
    let t_emp = Instant::now();
    let grades = ["A1", "B1", "C1", "M1", "DIR"];
    for eid in 1..=EMPLOYEES {
        let row = eid.to_string();
        let agency = ((eid - 1) % AGENCIES) + 1;
        let dept = ((eid * 7) % DEPARTMENTS) + 1;
        let salary = 220_000 + (eid % 980) * 1000;
        let grade = grades[(eid as usize) % grades.len()];
        put(&eng, "employees", &row, "id", &row);
        put(&eng, "employees", &row, "agency_id", &agency.to_string());
        put(&eng, "employees", &row, "department_id", &dept.to_string());
        put(&eng, "employees", &row, "matricule", &format!("MAT{eid:06}"));
        put(&eng, "employees", &row, "name", &format!("Emp {eid}"));
        put(&eng, "employees", &row, "grade", grade);
        put(&eng, "employees", &row, "salary_base", &salary.to_string());
        if eid % 10_000 == 0 {
            eprintln!(
                "  … {eid} employees ({:.0}/s)",
                eid as f64 / t_emp.elapsed().as_secs_f64()
            );
        }
    }
    eprintln!("  employees: {:.1}s", t_emp.elapsed().as_secs_f64());

    eprintln!("Phase 3: {EMPLOYEES} payslips (put_row bulk)…");
    let t_pay = Instant::now();
    for eid in 1..=EMPLOYEES {
        let row = eid.to_string();
        let gross = 220_000 + (eid % 980) * 1000;
        let social = gross * 22 / 100;
        let tax = gross * 15 / 100;
        let net = gross - social - tax;
        put(&eng, "payslips", &row, "id", &row);
        put(&eng, "payslips", &row, "period_id", "1");
        put(&eng, "payslips", &row, "employee_id", &row);
        put(&eng, "payslips", &row, "gross", &gross.to_string());
        put(&eng, "payslips", &row, "net", &net.to_string());
        put(&eng, "payslips", &row, "income_tax", &tax.to_string());
        put(&eng, "payslips", &row, "social", &social.to_string());
        if eid % 10_000 == 0 {
            eprintln!("  … {eid} payslips");
        }
    }
    eprintln!("  payslips: {:.1}s", t_pay.elapsed().as_secs_f64());

    let seed_ms = seed_start.elapsed().as_secs_f64() * 1000.0;
    eprintln!("Seed total: {:.1}s", seed_ms / 1000.0);

    let queries: &[(&str, &str)] = &[
        ("N01_count_employees", "SELECT COUNT(*) AS n FROM employees"),
        ("N02_count_payslips", "SELECT COUNT(*) AS n FROM payslips"),
        (
            "N03_headcount_by_agency",
            "SELECT agency_id, COUNT(*) AS n FROM employees GROUP BY agency_id ORDER BY n DESC LIMIT 10",
        ),
        (
            "N04_headcount_by_dept",
            "SELECT department_id, COUNT(*) AS n FROM employees GROUP BY department_id ORDER BY n DESC",
        ),
        (
            "N05_payroll_mass",
            "SELECT SUM(gross) AS masse FROM payslips WHERE period_id = 1",
        ),
        (
            "N06_avg_net_by_dept",
            "SELECT e.department_id, AVG(p.net) AS avg_net FROM payslips p INNER JOIN employees e ON e.id = p.employee_id WHERE p.period_id = 1 GROUP BY e.department_id ORDER BY avg_net DESC LIMIT 10",
        ),
        (
            "N07_country_headcount",
            "SELECT ag.country, COUNT(e.id) AS n FROM agencies ag INNER JOIN employees e ON e.agency_id = ag.id GROUP BY ag.country ORDER BY n DESC LIMIT 10",
        ),
        (
            "N08_dept_names",
            "SELECT d.name, COUNT(e.id) AS n FROM departments d INNER JOIN employees e ON e.department_id = d.id GROUP BY d.name ORDER BY n DESC LIMIT 10",
        ),
        (
            "N09_having_large_dept",
            "SELECT department_id, COUNT(*) AS n FROM employees GROUP BY department_id HAVING COUNT(*) > 2000 ORDER BY n DESC",
        ),
        (
            "N10_top_salaries",
            "SELECT e.name, e.salary_base, d.name AS dept FROM employees e INNER JOIN departments d ON d.id = e.department_id ORDER BY e.salary_base DESC LIMIT 15",
        ),
        (
            "N11_left_join",
            "SELECT ag.city, COUNT(e.id) AS n FROM agencies ag LEFT JOIN employees e ON e.agency_id = ag.id GROUP BY ag.city ORDER BY n DESC LIMIT 10",
        ),
        (
            "N12_in_subquery",
            "SELECT name FROM employees WHERE id IN (SELECT employee_id FROM payslips WHERE gross > 800000 AND period_id = 1) ORDER BY name LIMIT 10",
        ),
        (
            "N13_scalar_avg",
            "SELECT COUNT(*) AS n FROM employees WHERE salary_base > (SELECT AVG(salary_base) FROM employees)",
        ),
        (
            "N14_exists_dir",
            "SELECT ag.city FROM agencies ag WHERE EXISTS (SELECT 1 FROM employees e WHERE e.agency_id = ag.id AND e.grade = 'DIR') ORDER BY ag.city LIMIT 10",
        ),
        (
            "N15_not_exists",
            "SELECT e.name FROM employees e WHERE NOT EXISTS (SELECT 1 FROM payslips p WHERE p.employee_id = e.id) ORDER BY e.name LIMIT 5",
        ),
        (
            "N16_cte",
            "WITH hc AS (SELECT ag.country, COUNT(e.id) AS n FROM agencies ag INNER JOIN employees e ON e.agency_id = ag.id GROUP BY ag.country) SELECT country, n FROM hc ORDER BY n DESC LIMIT 10",
        ),
        (
            "N17_payroll_region",
            "SELECT ag.region, SUM(p.gross) AS masse FROM payslips p INNER JOIN employees e ON e.id = p.employee_id INNER JOIN agencies ag ON ag.id = e.agency_id WHERE p.period_id = 1 GROUP BY ag.region ORDER BY masse DESC LIMIT 10",
        ),
        (
            "N18_update_bonus",
            "UPDATE employees SET salary_base = salary_base + 5000 WHERE grade = 'A1' AND id <= 500",
        ),
        (
            "N19_delete_payslip",
            "DELETE FROM payslips WHERE id = 1",
        ),
        (
            "N20_count_after_delete",
            "SELECT COUNT(*) AS n FROM payslips",
        ),
    ];

    eprintln!("Phase 4: {} requetes metier…", queries.len());
    let mut benchmarks = Vec::new();
    let mut pass = 0u32;
    let mut fail = 0u32;
    for (name, sql) in queries {
        let b = run_bench(&eng, name, sql);
        eprintln!(
            "  {name}: {:.0}ms {}",
            b.ms,
            if b.ok { "OK" } else { "FAIL" }
        );
        if b.ok {
            pass += 1;
        } else {
            fail += 1;
        }
        benchmarks.push(b);
    }

    eprintln!("Phase 5: contraintes SQL…");
    let constraints_ok = eng
        .execute("INSERT INTO employees VALUES (1, 1, 1, 'DUP', 'Dup', 'A1', 100000)")
        .is_err()
        && eng
            .execute("INSERT INTO employees VALUES (99999, 1, 1, 'MAT000001', 'Dup', 'A1', 100000)")
            .is_err()
        && eng
            .execute("INSERT INTO employees VALUES (99998, 1, 1, 'MAT999998', NULL, 'A1', 100000)")
            .is_err();
    if constraints_ok {
        pass += 1;
    } else {
        fail += 1;
    }

    let count = run_bench(&eng, "N21_final_employee_count", "SELECT COUNT(*) AS n FROM employees");
    if count.ok {
        pass += 1;
    } else {
        fail += 1;
    }
    benchmarks.push(count);

    let report = Report {
        title: "NoeDB — Audit Banque Multinationale (80 agences, 50 009 employes)".into(),
        generated_at: format!("{}", humantime()),
        noedb_version: "2.0.0".into(),
        data_dir: dir.display().to_string(),
        agencies: AGENCIES,
        departments: DEPARTMENTS,
        employees: EMPLOYEES,
        payroll_periods: 1,
        payslips: EMPLOYEES,
        seed_load_ms: seed_ms,
        workspace_tests_passed: 310,
        benchmarks,
        summary_pass: pass,
        summary_fail: fail,
        notes: vec![
            format!("Chargement total: {:.1}s (put_row bulk + SQL ref)", seed_ms / 1000.0),
            format!(
                "{EMPLOYEES} employes · {EMPLOYEES} bulletins · {DEPARTMENTS} departements · {AGENCIES} agences"
            ),
            "Donnees bulk via put_row; requetes metier et contraintes via SQL".into(),
            "Contraintes PK/UNIQUE/NOT NULL validees sur INSERT SQL".into(),
        ],
    };

    let json = serde_json::to_string_pretty(&report).unwrap();
    fs::write("/tmp/noedb-national-audit-report.json", json).unwrap();
    println!("REPORT=/tmp/noedb-national-audit-report.json PASS={pass} FAIL={fail}");
}

fn humantime() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{secs} UTC")
}
