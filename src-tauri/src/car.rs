use anyhow::{anyhow, bail, Result};
use cid::Cid;
use ciborium::value::Value as CborValue;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

// ── MASL types ───────────────────────────────────────────────────────────────
//
// Resources are stored as flat maps: "src" → CID string, other keys → HTTP
// header values.  This mirrors the MASL structure directly (headers are
// siblings of `src`, not nested under a "headers" key).

pub type Resource = HashMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Masl {
    pub name: String,
    pub resources: HashMap<String, Resource>,
    #[serde(default)]
    pub icons: Vec<Icon>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Icon {
    pub src: String,
    #[serde(default)]
    pub sizes: String,
    #[serde(default)]
    pub purpose: String,
}

// ── Tile content ─────────────────────────────────────────────────────────────

/// Parsed tile: keeps the file path + MASL + CAR roots + a CID→(offset, len)
/// index so individual blocks can be served by seeking into the file on demand.
#[derive(Debug)]
pub struct TileContent {
    pub path: PathBuf,
    pub masl: Masl,
    /// CAR header roots, preserved so they can be written back unchanged.
    pub roots: Vec<Cid>,
    /// CID (canonical string form) → (byte offset of block data, byte length)
    pub index: HashMap<String, (u64, u64)>,
}

impl TileContent {
    /// Read the raw bytes of the block identified by `cid_str`.
    pub fn read_block(&self, cid_str: &str) -> Result<Vec<u8>> {
        let &(offset, len) = self
            .index
            .get(cid_str)
            .ok_or_else(|| anyhow!("block not found for CID {cid_str}"))?;
        let mut f = File::open(&self.path)?;
        f.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len as usize];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }
}

// ── CAR parsing ──────────────────────────────────────────────────────────────

/// Parse a `.tile` (CARv1) file. Returns `TileContent` with MASL metadata and
/// a CID→offset index built from the file's blocks.
pub fn parse_tile(path: &Path) -> Result<TileContent> {
    let mut f = File::open(path)?;
    let mut data = Vec::new();
    f.read_to_end(&mut data)?;

    let mut pos = 0usize;

    // ── header ────────────────────────────────────────────────────────────
    let (header_len, n) = read_uvarint(&data[pos..])
        .ok_or_else(|| anyhow!("failed to read CAR header varint"))?;
    pos += n;

    let header_end = pos + header_len as usize;
    if header_end > data.len() {
        bail!("CAR header length exceeds file size");
    }

    let (masl, roots) = parse_header(&data[pos..header_end])?;
    pos = header_end;

    // ── blocks ────────────────────────────────────────────────────────────
    let mut index: HashMap<String, (u64, u64)> = HashMap::new();

    while pos < data.len() {
        let (block_len, n) = read_uvarint(&data[pos..])
            .ok_or_else(|| anyhow!("failed to read block varint at pos {pos}"))?;
        pos += n;

        if block_len == 0 {
            break;
        }

        let block_end = pos + block_len as usize;
        if block_end > data.len() {
            bail!("block extends beyond file at pos {pos}");
        }

        let (cid, cid_len) = read_cid(&data[pos..])
            .ok_or_else(|| anyhow!("failed to parse CID at pos {pos}"))?;

        let data_offset = (pos + cid_len) as u64;
        let data_len = (block_len as usize - cid_len) as u64;
        index.insert(cid.to_string(), (data_offset, data_len));

        pos = block_end;
    }

    Ok(TileContent { path: path.to_path_buf(), masl, roots, index })
}

// ── MASL extraction from CBOR header ─────────────────────────────────────────

/// Parse the CAR header CBOR, returning the MASL and the root CIDs.
fn parse_header(header_bytes: &[u8]) -> Result<(Masl, Vec<Cid>)> {
    let value: CborValue = ciborium::de::from_reader(header_bytes)
        .map_err(|e| anyhow!("CBOR decode error: {e}"))?;

    let map = match value {
        CborValue::Map(m) => m,
        _ => bail!("CAR header is not a CBOR map"),
    };

    let mut name: Option<String> = None;
    let mut resources: HashMap<String, Resource> = HashMap::new();
    let mut icons: Vec<Icon> = Vec::new();
    let mut description: Option<String> = None;
    let mut short_name: Option<String> = None;
    let mut theme_color: Option<String> = None;
    let mut background_color: Option<String> = None;
    let mut roots: Vec<Cid> = Vec::new();

    for (k, v) in &map {
        let key = cbor_to_string(k).unwrap_or_default();
        match key.as_str() {
            "name" => name = cbor_to_string(v),
            "description" => description = cbor_to_string(v),
            "short_name" => short_name = cbor_to_string(v),
            "theme_color" => theme_color = cbor_to_string(v),
            "background_color" => background_color = cbor_to_string(v),
            "resources" => resources = parse_resources(v)?,
            "icons" => icons = parse_icons(v)?,
            "roots" => {
                if let CborValue::Array(arr) = v {
                    for item in arr {
                        if let Some(cid) = cbor_to_cid(item) {
                            roots.push(cid);
                        }
                    }
                }
            }
            _ => {} // skip `version` and unknown fields
        }
    }

    Ok((
        Masl {
            name: name.ok_or_else(|| anyhow!("MASL missing `name` field"))?,
            resources,
            icons,
            description,
            short_name,
            theme_color,
            background_color,
        },
        roots,
    ))
}

fn parse_resources(v: &CborValue) -> Result<HashMap<String, Resource>> {
    let map = match v {
        CborValue::Map(m) => m,
        _ => bail!("`resources` is not a CBOR map"),
    };
    let mut out = HashMap::new();
    for (k, rv) in map {
        let path = cbor_to_string(k).ok_or_else(|| anyhow!("resource key is not a string"))?;
        out.insert(path, parse_resource(rv)?);
    }
    Ok(out)
}

/// A resource entry is a flat map: `"src"` → CID string, other keys → header
/// values.  This matches the MASL format where headers are siblings of `src`.
fn parse_resource(v: &CborValue) -> Result<Resource> {
    let map = match v {
        CborValue::Map(m) => m,
        _ => bail!("resource entry is not a CBOR map"),
    };

    let mut out: Resource = HashMap::new();

    for (k, rv) in map {
        let key = cbor_to_string(k).unwrap_or_default();
        let value = if key == "src" {
            cbor_to_cid_string(rv)
                .ok_or_else(|| anyhow!("resource `src` is not a CID"))?
        } else if let Some(s) = cbor_to_string(rv) {
            s
        } else {
            continue; // skip non-string header values
        };
        out.insert(key, value);
    }

    if !out.contains_key("src") {
        bail!("resource missing `src` field");
    }
    Ok(out)
}

fn parse_icons(v: &CborValue) -> Result<Vec<Icon>> {
    let arr = match v {
        CborValue::Array(a) => a,
        _ => bail!("`icons` is not a CBOR array"),
    };
    let mut out = Vec::new();
    for item in arr {
        let map = match item {
            CborValue::Map(m) => m,
            _ => continue,
        };
        let mut src: Option<String> = None;
        let mut sizes = String::new();
        let mut purpose = String::new();
        for (k, iv) in map {
            match cbor_to_string(k).unwrap_or_default().as_str() {
                "src" => src = cbor_to_string(iv),
                "sizes" => sizes = cbor_to_string(iv).unwrap_or_default(),
                "purpose" => purpose = cbor_to_string(iv).unwrap_or_default(),
                _ => {}
            }
        }
        if let Some(src) = src {
            out.push(Icon { src, sizes, purpose });
        }
    }
    Ok(out)
}

// ── Self-modifying tile write ─────────────────────────────────────────────────

/// Store `data` under the key `name` in the tile's self-storage area
/// (`/.well-known/web-tiles-storage/<name>`).
///
/// Rewrites the tile's CAR file in place:
/// 1. Computes a CIDv1 (raw, sha2-256) for the new data.
/// 2. Updates `tile.masl.resources` with the new/updated entry.
/// 3. Reads all existing blocks, skipping the previous block for this name.
/// 4. Appends the new block at the end.
/// 5. Writes the result atomically via a temp-file rename.
/// 6. Updates `tile.index` to match.
pub fn write_tile_data(tile: &mut TileContent, name: &str, data: Vec<u8>) -> Result<()> {
    // ── 1. Compute CIDv1 (raw codec, sha2-256) for the new data ───────────
    let hash = Sha256::digest(&data);
    let mh = multihash::Multihash::<64>::wrap(0x12, hash.as_ref())
        .map_err(|e| anyhow!("multihash error: {e}"))?;
    let new_cid = Cid::new_v1(0x55, mh);
    let new_cid_str = new_cid.to_string();

    // ── 2. Locate the previous block CID for this name (if any) ───────────
    let storage_path = format!("/.well-known/web-tiles-storage/{name}");
    let old_cid_str = tile
        .masl
        .resources
        .get(&storage_path)
        .and_then(|r| r.get("src"))
        .cloned();

    // ── 3. Update MASL in memory ───────────────────────────────────────────
    let mut resource: Resource = HashMap::new();
    resource.insert("src".to_string(), new_cid_str.clone());
    resource.insert("content-type".to_string(), "application/octet-stream".to_string());
    tile.masl.resources.insert(storage_path, resource);

    // ── 4. Collect all existing blocks, skipping the old one ──────────────
    let mut file_data = Vec::new();
    File::open(&tile.path)?.read_to_end(&mut file_data)?;

    let mut blocks: Vec<(Cid, &[u8])> = Vec::new();
    for (cid_str, &(offset, len)) in &tile.index {
        if Some(cid_str.as_str()) == old_cid_str.as_deref() {
            continue; // drop the old block for this name
        }
        let slice = &file_data[offset as usize..(offset + len) as usize];
        let cid = Cid::try_from(cid_str.as_str())
            .map_err(|e| anyhow!("invalid CID {cid_str}: {e}"))?;
        blocks.push((cid, slice));
    }

    // ── 5. Build the new CBOR header ──────────────────────────────────────
    let header_cbor = build_header_cbor(&tile.masl, &tile.roots);
    let mut header_bytes = Vec::new();
    ciborium::ser::into_writer(&header_cbor, &mut header_bytes)
        .map_err(|e| anyhow!("CBOR serialisation error: {e}"))?;

    // ── 6. Serialise to a buffer, tracking new block offsets ──────────────
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&encode_uvarint(header_bytes.len() as u64));
    out.extend_from_slice(&header_bytes);

    let mut new_index: HashMap<String, (u64, u64)> = HashMap::new();

    // Existing blocks (old data for this name already excluded above)
    for (cid, block_data) in &blocks {
        let cid_bytes = cid.to_bytes();
        out.extend_from_slice(&encode_uvarint((cid_bytes.len() + block_data.len()) as u64));
        out.extend_from_slice(&cid_bytes);
        let data_offset = out.len() as u64;
        out.extend_from_slice(block_data);
        new_index.insert(cid.to_string(), (data_offset, block_data.len() as u64));
    }

    // New block at the end
    {
        let cid_bytes = new_cid.to_bytes();
        out.extend_from_slice(&encode_uvarint((cid_bytes.len() + data.len()) as u64));
        out.extend_from_slice(&cid_bytes);
        let data_offset = out.len() as u64;
        out.extend_from_slice(&data);
        new_index.insert(new_cid_str, (data_offset, data.len() as u64));
    }

    // ── 7. Atomic write via temp file + rename ────────────────────────────
    let parent = tile.path.parent().unwrap_or(Path::new("."));
    let file_name = tile.path.file_name().and_then(|n| n.to_str()).unwrap_or("tile");
    let temp_path = parent.join(format!(".{file_name}.tmp"));
    std::fs::write(&temp_path, &out)?;
    std::fs::rename(&temp_path, &tile.path)?;

    // ── 8. Update the in-memory index ─────────────────────────────────────
    tile.index = new_index;

    Ok(())
}

// ── CAR write helpers ─────────────────────────────────────────────────────────

/// Encode `n` as an unsigned LEB128 varint.
fn encode_uvarint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
    out
}

/// Encode a `Cid` as the DAG-CBOR CID link format: `Tag(42, Bytes(0x00 || raw))`.
fn cid_to_cbor_link(cid: &Cid) -> CborValue {
    let mut bytes = vec![0x00u8]; // identity multibase prefix
    bytes.extend_from_slice(&cid.to_bytes());
    CborValue::Tag(42, Box::new(CborValue::Bytes(bytes)))
}

/// Convert a CID *string* to a DAG-CBOR CID link, returning `None` on error.
fn cid_str_to_cbor_link(s: &str) -> Option<CborValue> {
    Cid::try_from(s).ok().map(|c| cid_to_cbor_link(&c))
}

/// Serialise a `Masl` + roots back into the CARv1 header CBOR map.
fn build_header_cbor(masl: &Masl, roots: &[Cid]) -> CborValue {
    let mut map: Vec<(CborValue, CborValue)> = Vec::new();

    // Standard CARv1 fields
    map.push((CborValue::Text("version".into()), CborValue::Integer(1.into())));
    map.push((
        CborValue::Text("roots".into()),
        CborValue::Array(roots.iter().map(cid_to_cbor_link).collect()),
    ));

    // MASL fields
    map.push((CborValue::Text("name".into()), CborValue::Text(masl.name.clone())));
    if let Some(v) = &masl.description {
        map.push((CborValue::Text("description".into()), CborValue::Text(v.clone())));
    }
    if let Some(v) = &masl.short_name {
        map.push((CborValue::Text("short_name".into()), CborValue::Text(v.clone())));
    }
    if let Some(v) = &masl.theme_color {
        map.push((CborValue::Text("theme_color".into()), CborValue::Text(v.clone())));
    }
    if let Some(v) = &masl.background_color {
        map.push((CborValue::Text("background_color".into()), CborValue::Text(v.clone())));
    }

    // resources map — each "src" value is a DAG-CBOR CID link
    let resources_map: Vec<(CborValue, CborValue)> = masl
        .resources
        .iter()
        .map(|(path, resource)| {
            let res_pairs: Vec<(CborValue, CborValue)> = resource
                .iter()
                .map(|(k, v)| {
                    let v_cbor = if k == "src" {
                        cid_str_to_cbor_link(v)
                            .unwrap_or_else(|| CborValue::Text(v.clone()))
                    } else {
                        CborValue::Text(v.clone())
                    };
                    (CborValue::Text(k.clone()), v_cbor)
                })
                .collect();
            (CborValue::Text(path.clone()), CborValue::Map(res_pairs))
        })
        .collect();
    map.push((CborValue::Text("resources".into()), CborValue::Map(resources_map)));

    // icons array (src here is a plain string path, not a CID link)
    if !masl.icons.is_empty() {
        let icons: Vec<CborValue> = masl
            .icons
            .iter()
            .map(|icon| {
                let mut pairs: Vec<(CborValue, CborValue)> =
                    vec![(CborValue::Text("src".into()), CborValue::Text(icon.src.clone()))];
                if !icon.sizes.is_empty() {
                    pairs.push((
                        CborValue::Text("sizes".into()),
                        CborValue::Text(icon.sizes.clone()),
                    ));
                }
                if !icon.purpose.is_empty() {
                    pairs.push((
                        CborValue::Text("purpose".into()),
                        CborValue::Text(icon.purpose.clone()),
                    ));
                }
                CborValue::Map(pairs)
            })
            .collect();
        map.push((CborValue::Text("icons".into()), CborValue::Array(icons)));
    }

    CborValue::Map(map)
}

// ── CBOR helpers ──────────────────────────────────────────────────────────────

fn cbor_to_string(v: &CborValue) -> Option<String> {
    match v {
        CborValue::Text(s) => Some(s.clone()),
        _ => None,
    }
}

/// Extract a CID from a DAG-CBOR CID link: `Tag(42, Bytes(0x00 || raw_cid))`.
/// The leading `0x00` byte is the identity multibase prefix.
fn cbor_to_cid_string(v: &CborValue) -> Option<String> {
    cbor_to_cid(v).map(|c| c.to_string())
}

fn cbor_to_cid(v: &CborValue) -> Option<Cid> {
    match v {
        CborValue::Tag(42, inner) => {
            if let CborValue::Bytes(bytes) = inner.as_ref() {
                let raw = if bytes.first() == Some(&0x00) { &bytes[1..] } else { bytes };
                Cid::try_from(raw).ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

// ── Varint / CID helpers ──────────────────────────────────────────────────────

/// Decode an unsigned LEB128 varint. Returns `(value, bytes_consumed)`.
fn read_uvarint(data: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for (i, &byte) in data.iter().enumerate() {
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// Parse a CID from the start of a slice. Returns `(cid, bytes_consumed)`.
fn read_cid(data: &[u8]) -> Option<(Cid, usize)> {
    let mut cursor = std::io::Cursor::new(data);
    let cid = Cid::read_bytes(&mut cursor).ok()?;
    Some((cid, cursor.position() as usize))
}

// ── Authority helper ──────────────────────────────────────────────────────────

/// Derive a `tile:` URI authority from the full file name.
/// e.g. `"My Document.tile"` → `"my-document.tile"`.
pub fn authority_from_path(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tile");
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
