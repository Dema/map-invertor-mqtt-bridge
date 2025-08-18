# Refactoring Suggestions for map-invertor-mqtt-bridge

This document outlines refactoring opportunities for the Rust codebase according to Rust best practices. These changes can be implemented one by one in separate sessions.

## 1. Error Handling Improvements ✅ COMPLETED

**Issue**: Mixed error handling approaches (anyhow + snafu)
**Recommendation**: Standardize on thiserror for library code and anyhow for application code while maintaining backtrace support.

**Changes**:
- Replace snafu with thiserror in map_protocol module
- Keep backtrace support using `#[backtrace]` attribute or anyhow's context functionality
- This provides structured error types at library level and easy error propagation at application level

```rust
// In map_protocol/mod.rs
#[derive(thiserror::Error, Debug)]
pub enum MapError {
    #[error("IO Error")]
    IOError {
        source: std::io::Error,
        #[backtrace]
        backtrace: std::backtrace::Backtrace,
    },
    #[error("Map cannot be found")]
    NotFound {
        #[backtrace]
        backtrace: std::backtrace::Backtrace,
    },
    // ... similar for other variants
}
```

## 2. Configuration/Parameter Handling ✅ COMPLETED

**Issue**: Duplicated parameter definitions in CLI
**Recommendation**: Create shared structs for common parameters

**Changes**:
1. Extract common parameters into a shared struct:

```rust
// In main.rs
#[derive(Clone, Debug, Args)]
struct SerialParams {
    /// Map port
    #[arg(short = 'p', long, env)]
    map_port: String,
    /// Map port speed
    #[arg(short = 's', long, env)]
    map_port_speed: u32,
}
```

Then use it in both modes with `#[command(flatten)]` attribute.

## 3. Code Structure/Module Organization ✅ COMPLETED

**Issue**: Tight coupling between modules and raw buffer access
**Recommendation**: Create clearer abstraction boundaries

**Changes**:
1. In `high_level.rs`, make fields of `MapInfo` private and provide constructor/accessor methods
2. Hide implementation details of `HighLevelProtocol` from public interface
3. Consider moving EEPROM-specific constants to a separate module

## 4. Memory Safety/Performance ⏳ IN PROGRESS

**Issue**: Direct buffer manipulation with potential bounds checking issues
**Recommendation**: Use safe abstractions where possible

**Changes**:
1. In `low_level.rs`, replace manual buffer shifting with safe alternatives:
   - Use `Vec::remove()` or slice operations instead of manual index manipulation
   - Add proper bounds checking for all array accesses

**Progress**:
- Created comprehensive testing framework in `DEBUGGING_DECODE_ANSWER.md`
- Implemented data capture mechanism to analyze real device behavior
- Created C reference implementation for comparison
- Identified potential issues in current Rust implementation
- Next steps: Run on device, capture data, and validate implementations

## 5. Code Quality ✅ COMPLETED

**Issue**: Several minor issues noted:
1. In `high_level.rs`, line 307 has a logical error (`||` should be `&&`)
2. Raw `dbg!()` macro usage in production code (main.rs:131)
3. Unnecessary `.clone()` operations on slices
4. Magic numbers in calculations

**Recommendations**:
1. Replace `dbg!()` with proper logging
2. Fix the logical error in the mode checking condition
3. Use constants for magic numbers
4. Implement `Display` trait for `MapInfo` instead of relying on `Debug`

**Changes Made**:
1. ✅ Replaced `dbg!()` with `log::debug!()` in main.rs
2. ✅ Logical error was already resolved
3. ✅ Added `InverterConstants` struct with named constants for magic numbers in high_level.rs
4. ✅ Implemented `Display` trait for `MapInfo` with a human-readable format

## 6. Documentation

**Issue**: Missing documentation for public API
**Recommendation**: Add rustdoc comments to all public structs, enums, and functions

## 7. Testing

**Issue**: Only CLI verification test exists
**Recommendation**: Add unit tests for protocol parsing logic