//! Platform-specific iCloud detection for macOS.
//!
//! Encodes assumptions about iCloud file locations as named constants.
//! Provides detection with explicit state representation.
//!
//! Structure:
//! - Constants: known path components (documented invariants)
//! - Types: possible detection states (sum type)
//! - Pure functions: path construction
//! - Effect functions: filesystem detection

use std::path::{Path, PathBuf};

// ============================================================================
// CONSTANTS (Documented Invariants)
// ============================================================================

/// Relative path from home to iCloud container.
///
/// This has been stable since macOS 10.8 Mountain Lion (2012).
/// All iCloud-synced content lives under this directory.
pub const ICLOUD_CONTAINER_REL: &str = "Library/Mobile Documents";

/// iCloud Drive container identifier.
///
/// This is the bundle ID for iCloud Drive itself.
/// User's iCloud Drive files (including Desktop/Documents if enabled) live here.
pub const ICLOUD_DRIVE_BUNDLE: &str = "com~apple~CloudDocs";

/// Common app container prefixes for reference.
///
/// Not exhaustive — apps register their own containers.
pub mod app_containers {
    pub const PAGES: &str = "com~apple~Pages";
    pub const NUMBERS: &str = "com~apple~Numbers";
    pub const KEYNOTE: &str = "com~apple~Keynote";
    pub const PREVIEW: &str = "com~apple~Preview";
}

// ============================================================================
// TYPES (State Representation)
// ============================================================================

/// Detected iCloud configuration state.
///
/// Represents what we found on the filesystem.
/// Each variant carries the relevant paths for that state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ICloudState {
    /// Full iCloud Drive enabled.
    /// Container exists and Drive folder present.
    DriveEnabled {
        /// ~/Library/Mobile Documents
        container: PathBuf,
        /// ~/Library/Mobile Documents/com~apple~CloudDocs
        drive_root: PathBuf,
    },

    /// iCloud signed in but Drive disabled.
    /// Container exists (apps may sync) but no Drive folder.
    DriveDisabled {
        /// ~/Library/Mobile Documents
        container: PathBuf,
    },

    /// iCloud not configured.
    /// Container directory does not exist.
    NotConfigured {
        /// Path we expected to find
        expected: PathBuf,
    },
}

/// Error during iCloud detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectionError {
    /// Could not determine home directory.
    HomeNotFound,

    /// Expected path exists but is not a directory.
    NotADirectory { path: PathBuf },
}

// ============================================================================
// PURE FUNCTIONS (Path Construction)
// ============================================================================

/// Compute expected iCloud container path from home directory.
///
/// Pure function — no I/O.
pub fn icloud_container_path(home: &Path) -> PathBuf {
    home.join(ICLOUD_CONTAINER_REL)
}

/// Compute expected iCloud Drive root from container.
///
/// Pure function — no I/O.
pub fn icloud_drive_path(container: &Path) -> PathBuf {
    container.join(ICLOUD_DRIVE_BUNDLE)
}

/// Compute both paths from home directory.
///
/// Pure function — no I/O.
pub fn icloud_paths(home: &Path) -> (PathBuf, PathBuf) {
    let container = icloud_container_path(home);
    let drive = icloud_drive_path(&container);
    (container, drive)
}

// ============================================================================
// EFFECT FUNCTIONS (Detection)
// ============================================================================

/// Detect iCloud configuration state.
///
/// Checks filesystem to determine current iCloud setup.
/// Returns a state enum, not Option/bool — exhaustive handling required.
pub fn detect_icloud() -> Result<ICloudState, DetectionError> {
    let home = dirs::home_dir().ok_or(DetectionError::HomeNotFound)?;
    detect_icloud_with_home(&home)
}

/// Detect iCloud state given a home directory.
///
/// Separated for testability — can inject test home path.
pub fn detect_icloud_with_home(home: &Path) -> Result<ICloudState, DetectionError> {
    let (container, drive_root) = icloud_paths(home);

    // Check container exists
    if !container.exists() {
        return Ok(ICloudState::NotConfigured {
            expected: container,
        });
    }

    // Verify it's a directory
    if !container.is_dir() {
        return Err(DetectionError::NotADirectory { path: container });
    }

    // Check if Drive is enabled
    if drive_root.exists() && drive_root.is_dir() {
        Ok(ICloudState::DriveEnabled {
            container,
            drive_root,
        })
    } else {
        Ok(ICloudState::DriveDisabled { container })
    }
}

/// Get the best path to scan based on detected state.
///
/// Returns the container path if iCloud is configured.
/// Caller decides how to handle NotConfigured.
pub fn scan_root(state: &ICloudState) -> Option<&Path> {
    match state {
        ICloudState::DriveEnabled { container, .. } => Some(container),
        ICloudState::DriveDisabled { container } => Some(container),
        ICloudState::NotConfigured { .. } => None,
    }
}

// ============================================================================
// DISPLAY (User-Friendly Messages)
// ============================================================================

impl std::fmt::Display for ICloudState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ICloudState::DriveEnabled { container, .. } => {
                write!(f, "iCloud Drive enabled ({})", container.display())
            }
            ICloudState::DriveDisabled { container } => {
                write!(
                    f,
                    "iCloud signed in, Drive disabled ({})",
                    container.display()
                )
            }
            ICloudState::NotConfigured { expected } => {
                write!(
                    f,
                    "iCloud not configured (expected: {})",
                    expected.display()
                )
            }
        }
    }
}

impl std::fmt::Display for DetectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectionError::HomeNotFound => {
                write!(f, "Could not determine home directory")
            }
            DetectionError::NotADirectory { path } => {
                write!(f, "Expected directory, found file: {}", path.display())
            }
        }
    }
}

impl std::error::Error for DetectionError {}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- Pure function tests ---

    #[test]
    fn test_icloud_container_path() {
        let home = PathBuf::from("/Users/test");
        let container = icloud_container_path(&home);
        assert_eq!(container, PathBuf::from("/Users/test/Library/Mobile Documents"));
    }

    #[test]
    fn test_icloud_drive_path() {
        let container = PathBuf::from("/Users/test/Library/Mobile Documents");
        let drive = icloud_drive_path(&container);
        assert_eq!(
            drive,
            PathBuf::from("/Users/test/Library/Mobile Documents/com~apple~CloudDocs")
        );
    }

    #[test]
    fn test_icloud_paths() {
        let home = PathBuf::from("/Users/test");
        let (container, drive) = icloud_paths(&home);
        assert!(container.to_string_lossy().contains("Mobile Documents"));
        assert!(drive.to_string_lossy().contains("CloudDocs"));
    }

    // --- Detection tests (with mock filesystem) ---

    #[test]
    fn test_detect_not_configured() {
        let temp = TempDir::new().unwrap();
        // Empty home — no iCloud container
        let state = detect_icloud_with_home(temp.path()).unwrap();

        assert!(matches!(state, ICloudState::NotConfigured { .. }));
        assert!(scan_root(&state).is_none());
    }

    #[test]
    fn test_detect_drive_disabled() {
        let temp = TempDir::new().unwrap();
        let container = temp.path().join(ICLOUD_CONTAINER_REL);
        fs::create_dir_all(&container).unwrap();
        // Container exists, but no CloudDocs

        let state = detect_icloud_with_home(temp.path()).unwrap();

        assert!(matches!(state, ICloudState::DriveDisabled { .. }));
        assert!(scan_root(&state).is_some());
    }

    #[test]
    fn test_detect_drive_enabled() {
        let temp = TempDir::new().unwrap();
        let container = temp.path().join(ICLOUD_CONTAINER_REL);
        let drive = container.join(ICLOUD_DRIVE_BUNDLE);
        fs::create_dir_all(&drive).unwrap();

        let state = detect_icloud_with_home(temp.path()).unwrap();

        match &state {
            ICloudState::DriveEnabled { container: c, drive_root: d } => {
                assert!(c.exists());
                assert!(d.exists());
            }
            _ => panic!("Expected DriveEnabled, got {:?}", state),
        }
        assert!(scan_root(&state).is_some());
    }

    #[test]
    fn test_display_messages() {
        let not_configured = ICloudState::NotConfigured {
            expected: PathBuf::from("/test"),
        };
        assert!(not_configured.to_string().contains("not configured"));

        let disabled = ICloudState::DriveDisabled {
            container: PathBuf::from("/test"),
        };
        assert!(disabled.to_string().contains("Drive disabled"));

        let enabled = ICloudState::DriveEnabled {
            container: PathBuf::from("/test"),
            drive_root: PathBuf::from("/test/drive"),
        };
        assert!(enabled.to_string().contains("Drive enabled"));
    }

    #[test]
    fn test_constants_are_reasonable() {
        // Sanity checks on our invariants
        assert!(ICLOUD_CONTAINER_REL.contains("Mobile Documents"));
        assert!(ICLOUD_DRIVE_BUNDLE.contains("CloudDocs"));
        assert!(!ICLOUD_CONTAINER_REL.starts_with('/'));
        assert!(!ICLOUD_CONTAINER_REL.starts_with('~'));
    }
}
