#!/bin/bash
cd $(dirname $0)
cd ..
fastboot=META-INF/bin/fastboot/fastboot
fastboot_f=META-INF/bin/fastboot/fastboot_f
echo "=============================================="
echo "         OemPorts10T Fastboot Flasher         "
echo "                     LINUX                    "
echo "=============================================="
read -p "Format Data? (y/n): " CHOICE

codename1=alioth
codename2=aliothin
checkdevice=$($fastboot getvar product 2>&1 | grep "product:" | awk '{print $2}')

if [[ $checkdevice == $codename1 ]] || [[ $checkdevice == $codename2 ]]; then
    echo ""
else
    echo "This ROM is not compatible for your device! aborting..."
    exit 1
fi

super=myui.alioth
echo "Flashing partitions..."
$fastboot_f -d $super -o $super.img
$fastboot erase super > /dev/null 2>&1
$fastboot flash super $super.img
rm -rf $super.img

if [[ -f oemports10t/oem_cust ]]; then
    echo ""
    echo "Flashing cust..."
    $fastboot_f -d oemports10t/oem_cust -o oem_cust.img
    $fastboot flash cust oem_cust.img
    rm -rf oem_cust.img
fi

echo ""
echo "Flashing images..."
$fastboot flash boot_ab images/boot.img
$fastboot flash dtbo_ab images/dtbo.img
$fastboot flash vendor_boot_ab images/vendor_boot.img
$fastboot flash vbmeta_ab images/vbmeta.img
$fastboot flash vbmeta_system_ab images/vbmeta_system.img

echo ""
echo "=============================================="
echo "          ROM INSTALLED SUCCESSFULLY          "
if [[ $CHOICE == y ]]; then
    echo "               FORMATTING DATA                "            
    $fastboot erase userdata > /dev/null 2>&1
    $fastboot erase metadata > /dev/null 2>&1
    echo "                   REBOOTING                  "
    $fastboot set_active a > /dev/null 2>&1
    $fastboot reboot
    echo "=============================================="
elif [[ $CHOICE == n ]]; then
    echo "=============================================="
    echo "if you want to wipe /data without formatting internal storage, reboot to recovery and wipe manually"
    echo ""
fi
