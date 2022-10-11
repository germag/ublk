# ublk
Rust friendly library for userspace block driver (ublk)

This library allows the implementation of generic userspace block devices.

ublk aims to be minimal and misuse-resistant.

## Status
Work in progress

### Todo
- Control path
  - Documentation, currently is just a place holder.
  - API review, I'm not happy with `DeviceParams`.
  - Better errors.
    - Sadly the kernel driver returns the same error for different
      error situations. And in some cases we need to send multiple
      messages, e.g., we can only send `SetParams` if the device' state
      is `DEAD`, this requires to first send `GetDevInfo` to get the
      status.

- Data path
  - everything.
    
## License

This project is licensed under

- [The MIT License](https://opensource.org/licenses/MIT)
