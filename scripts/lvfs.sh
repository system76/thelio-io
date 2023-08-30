#!/usr/bin/env bash

set -e

if [ -z "$1" ]
then
    echo "$0 [model] [description]" >&2
    exit 1
fi
MODEL="$1"

if [ -z "$2" ]
then
    echo "$0 [model] [description]" >&2
    exit 1
fi
DESCRIPTION="$2"

BOOTLOADER_VID="2E8A" # Raspberry Pi
RUNTIME_VID="3384" # System76
case "${MODEL}" in
    "thelio_io_2")
        BOOTLOADER_PID="0003" # RP2040
        RUNTIME_PID="000B"
        ;;
    *)
        echo "$0: unknown model '${MODEL}'" >&2
        exit 1
        ;;
esac

echo "MODEL: ${MODEL}"
echo "DESCRIPTION: ${DESCRIPTION}"

BOOTLOADER_ID="USB\\VID_${BOOTLOADER_VID}&PID_${BOOTLOADER_PID}"
echo "BOOTLOADER_ID: ${BOOTLOADER_ID}"

BOOTLOADER_UUID="$(appstream-util generate-guid "${BOOTLOADER_ID}")"
echo "BOOTLOADER_UUID: ${BOOTLOADER_UUID}"

RUNTIME_ID="USB\\VID_${RUNTIME_VID}&PID_${RUNTIME_PID}"
echo "RUNTIME_ID: ${RUNTIME_ID}"

RUNTIME_UUID="$(appstream-util generate-guid "${RUNTIME_ID}")"
echo "RUNTIME_UUID: ${RUNTIME_UUID}"

make -C firmware distclean
make -C firmware "system76/${MODEL}:default"

VERSION_HEADER="firmware/.build/obj_system76_${MODEL}_default/src/version.h"

REVISION="$(grep QMK_VERSION "${VERSION_HEADER}" | cut -d '"' -f2)"
echo "REVISION: ${REVISION}"

DATE="$(grep QMK_BUILDDATE "${VERSION_HEADER}" | cut -d '"' -f2 | cut -d '-' -f1,2,3)"
echo "DATE: ${DATE}"

NAME="${MODEL}_${REVISION}"
echo "NAME: ${NAME}"

SOURCE="https://github.com/system76/thelio-io"
echo "SOURCE: ${SOURCE}"

BUILD="build/lvfs/${NAME}"
echo "BUILD: ${BUILD}"

rm -rf "${BUILD}"
mkdir -pv "${BUILD}"

cp "firmware/.build/system76_${MODEL}_default.uf2" "${BUILD}/firmware.uf2"

echo "writing '${BUILD}/firmware.metainfo.xml'"
cat > "${BUILD}/firmware.metainfo.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!-- Copyright 2023 System76 <info@system76.com> -->
<component type="firmware">
  <id>com.system76.${MODEL}.firmware</id>
  <name>Thelio Io</name>
  <summary>System76 Thelio Io Firmware</summary>
  <description>
    <p>
      The System76 Thelio Io firmware is based on QMK and provides power button
      and fan control functionality
    </p>
  </description>
  <provides>
    <!-- ${RUNTIME_ID} -->
    <firmware type="flashed">${RUNTIME_UUID}</firmware>
  </provides>
  <url type="homepage">https://github.com/system76/thelio-io</url>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-2.0+</project_license>
  <developer_name>System76</developer_name>
  <releases>
    <release urgency="high" version="${REVISION}" date="${DATE}" install_duration="15">
      <checksum filename="firmware.uf2" target="content"/>
      <url type="source">${SOURCE}</url>
      <description>
        <p>${DESCRIPTION}</p>
      </description>
    </release>
  </releases>
  <requires>
    <id compare="ge" version="1.9.5">org.freedesktop.fwupd</id>
  </requires>
  <categories>
    <category>X-Device</category>
  </categories>
  <keywords>
    <keyword>uf2</keyword>
  </keywords>
  <custom>
    <value key="LVFS::UpdateProtocol">com.microsoft.uf2</value>
    <value key="LVFS::VersionFormat">plain</value>
  </custom>
</component>
EOF

gcab \
    --verbose \
    --create \
    --nopath \
    "${BUILD}.cab" \
    "${BUILD}/"*

echo "created '${BUILD}.cab'"
