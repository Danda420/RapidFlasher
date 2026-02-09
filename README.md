# RapidFlasher
A high-performance Android update-binary. Written in Rust, this binary is designed to be included in Android ROM Flashable ZIP packages to handle installation logic, partition flashing, and dynamic partition management with modern features like ZSTD decompression and multi-threaded I/O.

***
## Features ##
- Speed: Multi-threaded buffered writing for faster extraction and flashing.
- Compression Support: Native support for ZSTD and GZIP compressed images.
- Sparse Images: Native handling of Android Sparse images (normal and sparsechunk).
- Dynamic Partitions: Built-in logic to resize, unmap, and create logical partitions (super partition) using static lptools binaries.
- Simplified Scripting: parses a shell-like updater-script.
- AVB Control: Ability to disable vbmeta verification on the fly.

***
## Usage ##
To use this binary, you must structure your ZIP file correctly. The Android Recovery expects the binary to be located at `META-INF/com/google/android/update-binary`
````
Flashable.zip
└── META-INF
    └── com
        └── google
            └── android
                ├── update-binary      <-- This compiled binary
                └── updater-script     <-- Your installation instructions
````
or you can just check/use `FLASHABLE_ZIP_TEMPLATE` folder.

### Supported Commands ###
Below is the list of commands you can use in your updater-script.

| Command                     | Arguments                | Description
|-----------------------------|--------------------------|--------------------------------------------------------------------------------------------------------------|
| `ui_print`                  | `<message>`              | Prints a message to the recovery screen.                                                                     |
| `show_progress`             | `<fraction> <secs>`      | Updates the recovery progress bar.                                                                           |
| `verify_device`             | `device1,device2,...`    | Aborts installation if the device model (`ro.product.device` or `ro.build.product`) does not match the list. |
| `package_extract_file`      | `<file> <dest_path>`     | Extracts a single file from the ZIP to the system.                                                           |
| `package_flash_partition`   | `<method> <file> <dest>` | Flashes an image to a partition. See Flash Methods below.                                                    |
| `package_extract_targz`     | `<file> <dest_dir>`      | Extracts a GZIP-compressed tar archive from the ZIP to a directory.                                          |
| `update_dynamic_partitions` | `<op_list_file>`         | Modifies logical partitions based on a config file inside the ZIP.                                           |
| `set_slot`                  | `<slot>` *(0/1)*         | Sets the active boot slot using bootctl.                                                                     |
| `disable_vbmeta `           | *(none)*                 | Disables AVB verification (verity/verification) using avbctl.                                                |

### Flash Methods ###
When using package_flash_partition, the first argument determines how the source file is handled:
- 0 (ZSTD): Decompresses a ZSTD file stream directly to the partition.
- 1 (GZIP): Decompresses a GZIP file stream directly to the partition.
- 2 (Sparse): Flashes an Android Sparse Image.
    Auto-split detection: If the file in the zip ends in `.*`, the binary will automatically find and flash split chunks (e.g., `system.img.001`, `system.img.002`,...).

### Dynamic Partitions Guide ###
To resize or modify logical partitions, create a text file (e.g., dynamic_partitions_op_list) in your ZIP and call it via the script:
```` shell
update_dynamic_partitions "dynamic_partitions_op_list"
````
for dynamic_partitions_op_list format, refer to this [README](op_list.md)

Note: This feature requires lpdump, lpmake, and lptools binaries to be present in META-INF/bin/lptools/ inside the ZIP.

### Example (package_flash_partition package_extract_file package_extract_targz) ###
```` shell
# Flash super image (ZSTD)
package_flash_partition "0" "super.img.zst" "/dev/block/bootdevice/by-name/super"
# Flash super image (GZIP)
package_flash_partition "1" "super.img.gz" "/dev/block/bootdevice/by-name/super"
# Flash super image (Sparse)
package_flash_partition "2" "super.img" "/dev/block/bootdevice/by-name/super"
# Flash super image (Sparsechunk)
package_flash_partition "2" "super.img.*" "/dev/block/bootdevice/by-name/super"

# Flash boot.img to boot partition to current active slot (yea it could do that. usual values are _a/_b)
package_extract_file "boot.img" "/dev/block/bootdevice/by-name/boot${SLOT}"

# Extract tar.gz to a directory
package_extract_targz "oplus.tar.gz" "/data/oplus-partitions"
````

## Note ##
You can remove `META-INF/bin/lptools` if you don't plan on using `dynamic_partitions_op_list`