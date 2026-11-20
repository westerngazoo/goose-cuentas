//! `cuentas stats post` with the retention flags (`--salto`, `--medio`,
//! `--nosegu`), end to end: the real binary, a throwaway vault and HOME.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A vault with one account (`rotor`) whose repo has one post, under a fresh
/// temporary folder that also serves as HOME (nothing touches the real one).
fn mundo(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("goose-cuentas-cli-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let repo = d.join("repo");
    std::fs::create_dir_all(repo.join("posts/deriva")).unwrap();
    std::fs::write(
        repo.join("guion-cuentas.toml"),
        "[[cuenta]]\nnombre = \"Rotor\"\nposts = [\"posts/*\"]\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("posts/deriva/README.md"),
        "# ¿Qué es la deriva?\n\n- **estado:** publicado (2026-11-01)\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.join("vault")).unwrap();
    std::fs::create_dir_all(d.join("home")).unwrap();
    let ini = cuentas(&d, &["iniciar"]);
    assert!(ini.status.success(), "{}", texto(&ini));
    std::fs::write(
        d.join("vault/cuentas/cuentas.toml"),
        format!(
            "version = 1\n[[cuenta]]\nslug = \"rotor\"\nnombre = \"Rotor\"\ninstagram = \"rotorfisicoracing\"\nrepo = {:?}\n",
            repo.display().to_string()
        ),
    )
    .unwrap();
    d
}

fn cuentas(d: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cuentas"))
        .arg("--vault")
        .arg(d.join("vault"))
        .args(args)
        .env("HOME", d.join("home"))
        .env_remove("GOOSE_VAULT")
        .env_remove("USERPROFILE")
        .output()
        .unwrap()
}

fn texto(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn posts_csv(d: &Path) -> String {
    std::fs::read_to_string(d.join("vault/cuentas/estadisticas/posts.csv")).unwrap()
}

/// Runs `stats post rotor deriva <flag> <valor>` and returns the last CSV row.
fn medir(nombre: &str, flag: &str, valor: &str) -> (PathBuf, String) {
    let d = mundo(nombre);
    let o = cuentas(
        &d,
        &[
            "stats",
            "post",
            "rotor",
            "deriva",
            "--fecha",
            "2026-11-20",
            flag,
            valor,
        ],
    );
    assert!(o.status.success(), "{}", texto(&o));
    let csv = posts_csv(&d);
    let ultima = csv.lines().last().unwrap().to_string();
    (d, ultima)
}

/// Runs it with a bad value: it fails with the flag named and writes nothing.
fn rechaza(nombre: &str, flag: &str, valor: &str, motivo: &str) {
    let d = mundo(nombre);
    let antes = posts_csv(&d);
    let o = cuentas(
        &d,
        &[
            "stats", "post", "rotor", "deriva", "--likes", "3", flag, valor,
        ],
    );
    assert!(!o.status.success(), "{}", texto(&o));
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains(flag) && e.contains(motivo), "{e}");
    assert_eq!(posts_csv(&d), antes);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn salto_is_stored_as_a_percentage() {
    let (d, fila) = medir("salto-ok", "--salto", "45.5");
    assert_eq!(fila, "2026-11-20,rotor,deriva,,,,,,,,,45.5,,");
    let ver = cuentas(&d, &["stats", "ver", "rotor"]);
    assert!(
        texto(&ver).contains("salto 45.5\u{202f}% · medio — · no seg. —"),
        "{}",
        texto(&ver)
    );
    let tablero = cuentas(&d, &["tablero"]);
    assert!(
        texto(&tablero).contains("salto 45.5\u{202f}%"),
        "{}",
        texto(&tablero)
    );
    let nota = std::fs::read_to_string(d.join("vault/cuentas/resumen.md")).unwrap();
    assert!(nota.contains("| 45.5\u{202f}% | — | — |"), "{nota}");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn medio_is_stored_in_seconds() {
    let (d, fila) = medir("medio-ok", "--medio", "8");
    assert_eq!(fila, "2026-11-20,rotor,deriva,,,,,,,,,,8.0,");
    let ver = cuentas(&d, &["stats", "ver", "--json"]);
    assert!(texto(&ver).contains("\"medio_s\": 8.0"), "{}", texto(&ver));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn nosegu_is_stored_as_a_percentage() {
    let (d, fila) = medir("nosegu-ok", "--nosegu", "100");
    assert_eq!(fila, "2026-11-20,rotor,deriva,,,,,,,,,,,100.0");
    let ver = cuentas(&d, &["stats", "ver", "rotor"]);
    assert!(
        texto(&ver).contains("no seg. 100\u{202f}%"),
        "{}",
        texto(&ver)
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn salto_rejects_more_than_100_and_non_numbers() {
    rechaza("salto-alto", "--salto", "100.5", "fuera de rango");
    rechaza("salto-texto", "--salto", "45%", "no es un número");
}

#[test]
fn medio_rejects_negative_seconds_and_non_numbers() {
    rechaza("medio-neg", "--medio", "-1", "fuera de rango");
    rechaza("medio-texto", "--medio", "8s", "no es un número");
}

#[test]
fn nosegu_rejects_negative_and_non_finite() {
    rechaza("nosegu-neg", "--nosegu", "-0.1", "fuera de rango");
    rechaza("nosegu-nan", "--nosegu", "NaN", "no es un número");
}
