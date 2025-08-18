# Debugging the decode_answer Function

## Current Status

We're investigating potential issues in the `decode_answer` function in `src/map_protocol/low_level.rs`. This function is responsible for decoding escaped sequences in the MAP inverter protocol:

- `0xDB 0xDC` should be decoded to `0x0A`
- `0xDB 0xDD` should be decoded to `0xDB`

## Issue Identification

The current Rust implementation has a potential bug in the element shifting logic. When decoding a sequence, it should properly shift the remaining buffer elements to fill the gap left by the decoded sequence.

### Current Rust Implementation (Potentially Buggy)
```rust
fn decode_answer(&mut self) {
    let mut idx = 1;
    while idx <= self.last_read_bytes_index {
        {
            if self.buffer[idx] == 0xDB && self.buffer[idx + 1] == 0xDC {
                self.buffer[idx] = 0x0A;
                self.last_read_bytes_index -= 1;
                // Manual shifting - potentially buggy
                for i in idx..self.last_read_bytes_index - 1 {
                    self.buffer[i + 1] = self.buffer[i + 2];
                }
            } else if self.buffer[idx] == 0xDB && self.buffer[idx + 1] == 0xDD {
                self.buffer[idx] = 0xDB;
                self.last_read_bytes_index -= 1;
                // Manual shifting - potentially buggy
                for i in idx..self.last_read_bytes_index - 1 {
                    self.buffer[i + 1] = self.buffer[i + 2];
                }
            }
            idx += 1;
        }
    }
}
```

### Original C Implementation (Known Working)
```c
void decode_answer(int i)
{
    int c, c1;
    for (c = 1; c <= i; c++)
    {
        if (Buffer[c] == 0xDB && Buffer[c + 1] == 0xDC)
        {
            Buffer[c] = 0x0A;
            --i;
            for (c1 = c; c1 < (i - 1); c1++)
                Buffer[c1 + 1] = Buffer[c1 + 2];
        }
        else if (Buffer[c] == 0xDB && Buffer[c + 1] == 0xDD)
        {
            Buffer[c] = 0xDB;
            --i;
            for (c1 = c; c1 < (i - 1); c1++)
                Buffer[c1 + 1] = Buffer[c1 + 2];
        }
    }
}
```

## Testing Approach

We're implementing a comprehensive testing approach to validate both implementations:

1. **Data Capture**: Added logging to capture real buffer data from the device
2. **File Storage**: Saving raw and decoded buffer data to `/tmp/map_buffer_raw.bin` and `/tmp/map_buffer_decoded.bin`
3. **C Reference Implementation**: Created `test_decode.c` to run the known-working C implementation
4. **Comparison Framework**: Will compare the results of both implementations

## Next Steps

1. Compile and run the Rust program on the Orange Pi to capture real data
2. Use the C test program to process the captured data
3. Compare results to identify any discrepancies
4. Fix the Rust implementation if needed
5. Implement safer alternatives using `Vec::remove()` or `copy_within()`

## Files Created

- `test_decode.c`: C program to test the reference implementation
- `/tmp/map_buffer_raw.bin`: Raw buffer data from device (to be captured)
- `/tmp/map_buffer_decoded.bin`: Decoded buffer data from current Rust implementation (to be captured)

## Build Instructions

```bash
# Compile C test program
gcc -o test_decode test_decode.c

# Run comparison (after capturing data)
./test_decode /tmp/map_buffer_raw.bin /tmp/map_buffer_c_decoded.bin
```