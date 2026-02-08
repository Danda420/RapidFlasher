How you write dynamic_partition_op_list depends heavily on whether the device is A-only or A/B, and whether you want to completely wipe the super partition or just resize existing logical partitions.

***
## Usage ##
- `auto_detect_active_slot`: Logic flag. If present, the binary appends the active slot suffix (`_a` or `_b`) to every partition and group name defined in the file.
- `remove_all_groups`: Logic flag. If present, the binary will attempt to unmap and remove all existing dynamic partitions before creating new ones (Clean Flash).
- `add_group <name> <max_size>`: Defines a group (e.g., qti_dynamic_partitions) and its maximum size in bytes.
- `add <partition> <group>`: Adds a logical partition and assigns it to a group.
- `resize <partition> <size>`: Sets the size (in bytes) for a partition.

### A/B Device (Manual Mode) ###
````
remove_all_groups

# You must explicitly define groups for specific slots
add_group qti_dynamic_partitions_a 9124708352
add_group qti_dynamic_partitions_b 9124708352

# Explicitly add partitions for Slot A
add system_a qti_dynamic_partitions_a
add vendor_a qti_dynamic_partitions_a

# Explicitly add partitions for Slot B
add system_b qti_dynamic_partitions_b
add vendor_b qti_dynamic_partitions_b

resize system_b 1073741824
resize vendor_b 1073741824
````

### A/B Device (Auto-Detect Mode) ###
````
auto_detect_active_slot
remove_all_groups

# Define base group name (Binary converts this to "qti_dynamic_partitions_a" if current active slot is "_a")
add_group qti_dynamic_partitions 9124708352

# Define base partition names (Binary converts to "system_a", "vendor_a", etc.)
add system qti_dynamic_partitions
add vendor qti_dynamic_partitions
add product qti_dynamic_partitions

# Resize (Binary converts to "system_a", etc.)
resize system 1073741824
resize vendor 536870912
resize product 536870912
````

### A-Only Device ###
````
remove_all_groups

# Standard names without suffixes
add_group qti_dynamic_partitions 4294967296

add system qti_dynamic_partitions
add vendor qti_dynamic_partitions

resize system 2147483648
resize vendor 536870912
````

### Incremental Resize (No Wipe) ###
````
# No "remove_all_groups"
# No "add_group"
# No "add" (partition mapping)

# Just resize specific partitions
resize system 2684354560
resize vendor 838860800
````
Note: If using this on A/B, you usually need to specify the full name like `system_a` unless you also use `auto_detect_active_slot`