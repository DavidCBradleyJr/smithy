//! Just enough GGUF to read string metadata (the chat template) from a model
//! file without loading it. Arrays such as the token vocabulary are skipped
//! by seeking, so this reads a few MB at most even for large models.

use std::{
    fs::File,
    io::{self, BufReader, Read, Seek},
    path::Path,
};

const MAGIC: &[u8; 4] = b"GGUF";

// GGUF value types.
const STRING: u32 = 8;
const ARRAY: u32 = 9;

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn read_string(r: &mut impl Read) -> io::Result<String> {
    let len = read_u64(r)?;
    if len > 64 << 20 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "string too long"));
    }
    let mut b = vec![0u8; len as usize];
    r.read_exact(&mut b)?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}

/// Size in bytes of a fixed-size scalar type, or `None` for strings/arrays.
fn scalar_size(t: u32) -> Option<i64> {
    match t {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4 | 5 | 6 => Some(4),
        10 | 11 | 12 => Some(8),
        _ => None,
    }
}

fn skip_value<R: Read + Seek>(r: &mut R, t: u32) -> io::Result<()> {
    if let Some(n) = scalar_size(t) {
        return r.seek_relative(n);
    }
    match t {
        STRING => {
            let len = read_u64(r)?;
            r.seek_relative(len as i64)
        }
        ARRAY => {
            let et = read_u32(r)?;
            let count = read_u64(r)?;
            match scalar_size(et) {
                Some(n) => r.seek_relative(n * count as i64),
                None => {
                    for _ in 0..count {
                        skip_value(r, et)?;
                    }
                    Ok(())
                }
            }
        }
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown gguf type {t}"))),
    }
}

/// The string value of metadata `key`, if present.
pub fn read_string_key(path: &Path, key: &str) -> io::Result<Option<String>> {
    let mut r = BufReader::with_capacity(1 << 16, File::open(path)?);
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a GGUF file"));
    }
    let _version = read_u32(&mut r)?;
    let _tensors = read_u64(&mut r)?;
    let kvs = read_u64(&mut r)?;
    for _ in 0..kvs {
        let k = read_string(&mut r)?;
        let t = read_u32(&mut r)?;
        if k == key && t == STRING {
            return Ok(Some(read_string(&mut r)?));
        }
        skip_value(&mut r, t)?;
    }
    Ok(None)
}

pub fn chat_template(path: &Path) -> Option<String> {
    read_string_key(path, "tokenizer.chat_template").ok().flatten()
}

/// Reasoning-effort values a chat template accepts, when it validates them
/// (`reasoning_effort not in ('xhigh', 'medium', 'low')` and similar).
/// `None` means the template doesn't restrict them.
pub fn template_efforts(template: &str) -> Option<Vec<String>> {
    let at = template.find("reasoning_effort")?;
    let rest = &template[at..];
    let open = rest.find("not in (")? + "not in (".len();
    // Only trust a list that sits right next to the effort variable.
    if open > 200 {
        return None;
    }
    let close = rest[open..].find(')')?;
    let values: Vec<String> = rest[open..open + close]
        .split(',')
        .map(|v| v.trim().trim_matches(|c| c == '\'' || c == '"').to_string())
        .filter(|v| !v.is_empty())
        .collect();
    (!values.is_empty()).then_some(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn put_str(f: &mut Vec<u8>, s: &str) {
        f.extend_from_slice(&(s.len() as u64).to_le_bytes());
        f.extend_from_slice(s.as_bytes());
    }

    #[test]
    fn reads_template_past_arrays_and_scalars() {
        let mut f = Vec::new();
        f.extend_from_slice(b"GGUF");
        f.extend_from_slice(&3u32.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
        f.extend_from_slice(&4u64.to_le_bytes());
        // u32 scalar
        put_str(&mut f, "general.alignment");
        f.extend_from_slice(&4u32.to_le_bytes());
        f.extend_from_slice(&32u32.to_le_bytes());
        // array of strings (a vocabulary)
        put_str(&mut f, "tokenizer.ggml.tokens");
        f.extend_from_slice(&ARRAY.to_le_bytes());
        f.extend_from_slice(&STRING.to_le_bytes());
        f.extend_from_slice(&3u64.to_le_bytes());
        for t in ["a", "bb", "ccc"] {
            put_str(&mut f, t);
        }
        // array of f32
        put_str(&mut f, "tokenizer.ggml.scores");
        f.extend_from_slice(&ARRAY.to_le_bytes());
        f.extend_from_slice(&6u32.to_le_bytes());
        f.extend_from_slice(&2u64.to_le_bytes());
        f.extend_from_slice(&[0u8; 8]);
        put_str(&mut f, "tokenizer.chat_template");
        f.extend_from_slice(&STRING.to_le_bytes());
        put_str(&mut f, "{{ messages }}");

        let path = std::env::temp_dir().join(format!("smithy-gguf-{}.gguf", std::process::id()));
        std::fs::File::create(&path).unwrap().write_all(&f).unwrap();
        let t = chat_template(&path);
        let missing = read_string_key(&path, "nope").unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(t.as_deref(), Some("{{ messages }}"));
        assert_eq!(missing, None);
    }

    #[test]
    fn efforts_from_bonsai_style_template() {
        let t = "{%- set resolved_reasoning_effort = reasoning_effort|default('xhigh') %}\n    {%- if resolved_reasoning_effort not in ('xhigh', 'medium', 'low') %}\n{{ raise_exception('x') }}";
        assert_eq!(template_efforts(t), Some(vec!["xhigh".into(), "medium".into(), "low".into()]));
        assert_eq!(template_efforts("{{ messages }}"), None);
        // A `not in (` far away from the variable isn't about efforts.
        let far = format!("reasoning_effort {} not in ('a')", "x".repeat(500));
        assert_eq!(template_efforts(&far), None);
    }
}
