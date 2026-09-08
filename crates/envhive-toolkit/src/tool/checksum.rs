//! 完整性校验（roadmap D：SHA256 / SHA512 / SHA1 / MD5 多算法；"none" 跳过）

use std::io::Read;
use std::path::Path;

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgo {
    Sha256,
    Sha512,
    Sha1,
    Md5,
}

impl HashAlgo {
    pub fn from_str(s: &str) -> Option<HashAlgo> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sha256" => Some(HashAlgo::Sha256),
            "sha512" => Some(HashAlgo::Sha512),
            "sha1" => Some(HashAlgo::Sha1),
            "md5" => Some(HashAlgo::Md5),
            _ => None,
        }
    }
}

/// 校验说明：`"sha256:<hex>"` / `"none"` / 裸 hex（默认 sha256）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checksum {
    pub algo: HashAlgo,
    pub digest: String,
}

pub fn parse(spec: &str) -> Option<Checksum> {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some((algo, hex)) = spec.split_once(':') {
        if let Some(algo) = HashAlgo::from_str(algo) {
            return Some(Checksum { algo, digest: hex.trim().to_ascii_lowercase() });
        }
    }
    // 裸 hex 默认 sha256
    if spec.len() == 64 && spec.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(Checksum { algo: HashAlgo::Sha256, digest: spec.to_ascii_lowercase() });
    }
    None
}

/// 校验文件哈希与期望值一致；spec 为 "none"/空时跳过
pub fn verify_file(path: &Path, spec: &str) -> Result<()> {
    let Some(checksum) = parse(spec) else {
        tracing::warn!("{} 跳过校验（spec={spec:?}）", path.display());
        return Ok(());
    };
    let actual = file_hash(path, checksum.algo)?;
    if !actual.eq_ignore_ascii_case(&checksum.digest) {
        return Err(EnvHiveError::new(
            EnvHiveErrorKind::Checksum,
            format!(
                "{} 校验失败：期望 {}:{}, 实际 {}:{actual}",
                path.display(),
                algo_name(checksum.algo),
                checksum.digest,
                algo_name(checksum.algo)
            ),
        ));
    }
    tracing::info!("{} 校验通过（{}:{actual}）", path.display(), algo_name(checksum.algo));
    Ok(())
}

pub fn algo_name(algo: HashAlgo) -> &'static str {
    match algo {
        HashAlgo::Sha256 => "sha256",
        HashAlgo::Sha512 => "sha512",
        HashAlgo::Sha1 => "sha1",
        HashAlgo::Md5 => "md5",
    }
}

/// 计算文件哈希（hex 小写）
pub fn file_hash(path: &Path, algo: HashAlgo) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 64 * 1024];
    let digest_hex: String;

    macro_rules! feed {
        ($hasher:ident) => {{
            let mut hasher = $hasher::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            hex::encode(hasher.finalize())
        }};
    }

    digest_hex = match algo {
        HashAlgo::Sha256 => feed!(Sha256),
        HashAlgo::Sha512 => feed!(Sha512),
        HashAlgo::Sha1 => feed!(Sha1),
        HashAlgo::Md5 => feed!(Md5),
    };
    Ok(digest_hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        assert!(parse("none").is_none());
        assert!(parse("").is_none());
        let c = parse("sha256:abcdef").unwrap();
        assert_eq!(c.algo, HashAlgo::Sha256);
        let c2 = parse("sha512:xyz").unwrap();
        assert_eq!(c2.algo, HashAlgo::Sha512);
        // 64 位 hex → sha256
        let s = "a".repeat(64);
        assert_eq!(parse(&s).unwrap().algo, HashAlgo::Sha256);
    }
}
