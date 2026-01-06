# Port Documentation

## Overview
This document provides comprehensive documentation for all port types including usage examples, error cases, and implementation guidelines.

## SerialPort Trait

The `SerialPort` trait provides a common interface for serial port devices across different platforms.

### Methods

#### `name(&self) -> Option<String>`
Returns the name of the port if it exists.

**Example:**
```rust
let port_name = port.name();
println!("Port name: {:?}", port_name);
```

#### `baud_rate(&self) -> Result<u32>`
Returns the current baud rate.

**Example:**
```rust
match port.baud_rate() {
    Ok(rate) => println!("Current baud rate: {}", rate),
    Err(e) => eprintln!("Error getting baud rate: {}", e),
}
```

#### `set_baud_rate(&mut self, baud_rate: u32) -> Result<()>`
Sets the baud rate.

**Error Cases:**
- `InvalidInput` if the baud rate is not supported
- `NoDevice` if the device is disconnected
- `Io` for other I/O errors

**Example:**
```rust
if let Err(e) = port.set_baud_rate(9600) {
    eprintln!("Failed to set baud rate: {}", e);
}
```

#### `data_bits(&self) -> Result<DataBits>`
Returns the character size.

**Example:**
```rust
match port.data_bits() {
    Ok(bits) => println!("Data bits: {:?}", bits),
    Err(e) => eprintln!("Error getting data bits: {}", e),
}
```

## Implementation Guidelines

1. **Platform Specifics**: Implementations should handle platform-specific behaviors for serial port configuration.
2. **Error Handling**: Always check for device disconnection and invalid configurations.
3. **Thread Safety**: Ensure implementations are thread-safe if used in concurrent contexts.

## References
- [serialport-rs](https://docs.rs/serialport/latest/serialport/)
- [tokio-serial](https://docs.rs/tokio-serial/latest/tokio_serial/)
