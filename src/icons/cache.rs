//! Caché de iconos convertidos: en memoria durante la ejecución y en disco
//! (`~/.cache/apptop/`) entre ejecuciones.
//!
//! La clave incluye la ruta del icono, su mtime, su tamaño en bytes, el
//! tamaño en celdas y la versión del pipeline: si cualquiera cambia, el
//! icono se vuelve a convertir.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use super::braille::BrailleIcon;
use super::convert_file;
use super::generic::{generic_icon, GenericKind};
use super::locate::IconLocator;

/// Cambiar cuando cambie el resultado del pipeline, para invalidar la caché.
const PIPELINE_VERSION: u32 = 1;

const MAGIC: &[u8; 4] = b"APTI";

/// Directorio de caché: `$XDG_CACHE_HOME/apptop` o `~/.cache/apptop`.
pub fn default_cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|base| base.join("apptop"))
}

/// FNV-1a de 64 bits: estable entre versiones de Rust (a diferencia del
/// hasher por defecto), así los nombres de archivo sobreviven a recompilar.
fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ *byte as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Clave de caché de un archivo de icono, o `None` si no se puede leer.
fn cache_key(path: &Path, width: u16, height: u16) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(format!(
        "v{PIPELINE_VERSION}|{}|{}.{:09}|{}|{width}x{height}",
        path.display(),
        mtime.as_secs(),
        mtime.subsec_nanos(),
        meta.len()
    ))
}

fn encode(key: &str, icon: &BrailleIcon) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + key.len() + icon.cells.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&icon.width.to_le_bytes());
    out.extend_from_slice(&icon.height.to_le_bytes());
    out.extend_from_slice(&[icon.color.0, icon.color.1, icon.color.2]);
    out.extend_from_slice(&icon.cells);
    out
}

/// Decodifica un archivo de caché; `None` si está dañado o es de otra clave.
fn decode(data: &[u8], key: &str) -> Option<BrailleIcon> {
    let rest = data.strip_prefix(MAGIC)?;
    let (len, rest) = rest.split_first_chunk::<4>()?;
    let len = u32::from_le_bytes(*len) as usize;
    if rest.len() < len || &rest[..len] != key.as_bytes() {
        return None;
    }
    let rest = &rest[len..];
    let (head, cells) = rest.split_first_chunk::<7>()?;
    let width = u16::from_le_bytes([head[0], head[1]]);
    let height = u16::from_le_bytes([head[2], head[3]]);
    if cells.len() != width as usize * height as usize {
        return None;
    }
    Some(BrailleIcon {
        width,
        height,
        cells: cells.to_vec(),
        color: (head[4], head[5], head[6]),
    })
}

/// Entrega iconos convertidos, resolviendo nombre → archivo → braille con
/// caché en memoria y en disco.
pub struct IconStore {
    locator: IconLocator,
    dir: Option<PathBuf>,
    /// Nombre de icono → archivo (o `None` si el tema no lo tiene).
    located: HashMap<String, Option<PathBuf>>,
    /// (archivo, tamaño) → icono (o `None` si no se pudo convertir).
    files: HashMap<(PathBuf, u16, u16), Option<Arc<BrailleIcon>>>,
    generic: HashMap<(GenericKind, u16, u16), Arc<BrailleIcon>>,
    /// Cuántos iconos hubo que convertir (no estaban en ninguna caché).
    pub conversions: usize,
}

impl IconStore {
    /// Usa el tema activo y el directorio de caché por defecto.
    pub fn new() -> Self {
        Self::with(IconLocator::detect(), default_cache_dir())
    }

    /// `dir = None` desactiva la caché en disco.
    pub fn with(locator: IconLocator, dir: Option<PathBuf>) -> Self {
        Self {
            locator,
            dir,
            located: HashMap::new(),
            files: HashMap::new(),
            generic: HashMap::new(),
            conversions: 0,
        }
    }

    pub fn locator(&self) -> &IconLocator {
        &self.locator
    }

    /// Icono para un valor de `Icon=` (nombre o ruta), si existe y se puede
    /// convertir.
    pub fn named(&mut self, icon: &str, width: u16, height: u16) -> Option<Arc<BrailleIcon>> {
        let path = match self.located.get(icon) {
            Some(cached) => cached.clone(),
            None => {
                let found = self.locator.locate(icon);
                self.located.insert(icon.to_string(), found.clone());
                found
            }
        }?;
        self.file(&path, width, height)
    }

    /// Icono de un archivo concreto.
    pub fn file(&mut self, path: &Path, width: u16, height: u16) -> Option<Arc<BrailleIcon>> {
        let mem_key = (path.to_path_buf(), width, height);
        if let Some(cached) = self.files.get(&mem_key) {
            return cached.clone();
        }
        let icon = self.load_or_convert(path, width, height).map(Arc::new);
        self.files.insert(mem_key, icon.clone());
        icon
    }

    fn load_or_convert(&mut self, path: &Path, width: u16, height: u16) -> Option<BrailleIcon> {
        let key = cache_key(path, width, height)?;
        let cache_file = self
            .dir
            .as_ref()
            .map(|d| d.join(format!("{:016x}.icon", fnv1a(key.as_bytes()))));
        if let Some(file) = &cache_file {
            if let Some(icon) = fs::read(file).ok().and_then(|data| decode(&data, &key)) {
                return Some(icon);
            }
        }
        self.conversions += 1;
        let icon = convert_file(path, width, height)?;
        if let Some(file) = &cache_file {
            // La caché es opcional: si no se puede escribir, se sigue igual.
            let _ = write_atomic(file, &encode(&key, &icon));
        }
        Some(icon)
    }

    /// Icono genérico (solo en memoria: generarlo es barato).
    pub fn generic(&mut self, kind: GenericKind, width: u16, height: u16) -> Arc<BrailleIcon> {
        self.generic
            .entry((kind, width, height))
            .or_insert_with(|| Arc::new(generic_icon(kind, width, height)))
            .clone()
    }

    /// Icono con respaldo: el propio si existe, si no el genérico.
    pub fn named_or_generic(
        &mut self,
        icon: Option<&str>,
        fallback: GenericKind,
        width: u16,
        height: u16,
    ) -> Arc<BrailleIcon> {
        icon.and_then(|name| self.named(name, width, height))
            .unwrap_or_else(|| self.generic(fallback, width, height))
    }
}

impl Default for IconStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Escribe a un temporal y renombra, para no dejar archivos a medias.
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let result = fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(data))
        .and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::raster::tests::DISC_SVG;
    use std::time::{Duration, SystemTime};

    struct Fixture {
        tmp: tempfile::TempDir,
        svg: PathBuf,
        cache: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let svg = tmp.path().join("disc.svg");
        fs::write(&svg, DISC_SVG).unwrap();
        let cache = tmp.path().join("cache");
        Fixture { tmp, svg, cache }
    }

    fn store(f: &Fixture) -> IconStore {
        IconStore::with(IconLocator::with_theme("hicolor"), Some(f.cache.clone()))
    }

    fn cache_files(f: &Fixture) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = fs::read_dir(&f.cache)
            .map(|d| d.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        files
    }

    #[test]
    fn converts_once_then_serves_from_memory_and_disk() {
        let f = fixture();
        let mut first = store(&f);
        let a = first.file(&f.svg, 16, 8).unwrap();
        let b = first.file(&f.svg, 16, 8).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(first.conversions, 1);
        assert_eq!(cache_files(&f).len(), 1);

        // Otra ejecución: sale del disco sin convertir.
        let mut second = store(&f);
        let c = second.file(&f.svg, 16, 8).unwrap();
        assert_eq!(*c, *a);
        assert_eq!(second.conversions, 0);
    }

    #[test]
    fn size_and_mtime_are_part_of_the_key() {
        let f = fixture();
        let mut s = store(&f);
        s.file(&f.svg, 16, 8).unwrap();
        let big = s.file(&f.svg, 24, 12).unwrap();
        assert_eq!((big.width, big.height), (24, 12));
        assert_eq!(cache_files(&f).len(), 2);

        // Cambia el mtime: la entrada vieja deja de servir.
        let later = SystemTime::now() + Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(&f.svg)
            .unwrap()
            .set_modified(later)
            .unwrap();
        let mut fresh = store(&f);
        fresh.file(&f.svg, 16, 8).unwrap();
        assert_eq!(fresh.conversions, 1);
        assert_eq!(cache_files(&f).len(), 3);
    }

    #[test]
    fn corrupt_cache_files_are_ignored_and_rewritten() {
        let f = fixture();
        let original = store(&f).file(&f.svg, 16, 8).unwrap();
        let file = cache_files(&f).remove(0);
        fs::write(&file, b"APTIbasura").unwrap();
        let mut s = store(&f);
        assert_eq!(*s.file(&f.svg, 16, 8).unwrap(), *original);
        assert_eq!(s.conversions, 1);
        assert!(fs::read(&file).unwrap().len() > 20);
    }

    #[test]
    fn works_without_a_cache_dir_or_with_an_unwritable_one() {
        let f = fixture();
        let mut none = IconStore::with(IconLocator::with_theme("hicolor"), None);
        assert!(none.file(&f.svg, 16, 8).is_some());
        let blocked = f.tmp.path().join("archivo");
        fs::write(&blocked, b"x").unwrap();
        let mut bad = IconStore::with(
            IconLocator::with_theme("hicolor"),
            Some(blocked.join("sub")),
        );
        assert!(bad.file(&f.svg, 16, 8).is_some());
    }

    #[test]
    fn failures_are_remembered_and_fall_back_to_generic() {
        let f = fixture();
        let broken = f.tmp.path().join("roto.svg");
        fs::write(&broken, b"<svg").unwrap();
        let mut s = store(&f);
        assert!(s.file(&broken, 16, 8).is_none());
        assert!(s.file(&broken, 16, 8).is_none());
        assert_eq!(s.conversions, 1, "no se reintenta en cada refresco");

        let icon = s.named_or_generic(broken.to_str(), GenericKind::Service, 16, 8);
        assert_eq!(icon.color, GenericKind::Service.color());
        let none = s.named_or_generic(None, GenericKind::App, 16, 8);
        assert_eq!(none.color, GenericKind::App.color());
        let real = s.named_or_generic(f.svg.to_str(), GenericKind::App, 16, 8);
        assert_eq!(real.color, (0xe6, 0x60, 0x00));
    }

    #[test]
    fn encode_decode_round_trip_checks_the_key() {
        let icon = BrailleIcon {
            width: 2,
            height: 1,
            cells: vec![0x01, 0xff],
            color: (1, 2, 3),
        };
        let data = encode("clave", &icon);
        assert_eq!(decode(&data, "clave"), Some(icon));
        assert_eq!(decode(&data, "otra"), None);
        assert_eq!(decode(&data[..data.len() - 1], "clave"), None);
        assert_eq!(decode(b"", "clave"), None);
    }

    #[test]
    fn fnv_is_the_reference_implementation() {
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    }
}
