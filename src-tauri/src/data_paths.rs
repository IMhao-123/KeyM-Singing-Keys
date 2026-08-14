use std::path::{Path, PathBuf};

/// 测试/诊断覆盖环境变量名。设置后生产构造器改用该绝对路径，避免触碰真实用户目录。
pub const TEST_DATA_DIR_ENV: &str = "KEYM_TEST_DATA_DIR";

/// 应用数据根目录。优先读取 `KEYM_TEST_DATA_DIR` 覆盖；否则回退到系统数据目录。
pub fn data_root() -> Result<PathBuf, String> {
    match std::env::var_os(TEST_DATA_DIR_ENV) {
        Some(value) => parse_override(Path::new(&value)),
        None => dirs_next::data_dir().ok_or_else(|| "无法确定应用数据目录".to_string()),
    }
}

fn parse_override(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err(format!("{TEST_DATA_DIR_ENV} 不能为空"));
    }
    if !path.is_absolute() {
        return Err(format!("{TEST_DATA_DIR_ENV} 必须是绝对路径"));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_requires_an_absolute_nonempty_path() {
        assert!(parse_override(Path::new("")).is_err());
        assert!(parse_override(Path::new("relative/path")).is_err());
        assert_eq!(
            parse_override(Path::new("/tmp/keym-runtime-test")).unwrap(),
            PathBuf::from("/tmp/keym-runtime-test")
        );
    }
}
