.PHONY: all FORCE

all: output/firmware output/hardware

output/firmware: FORCE
	rm -rf "$@"
	mkdir -p "$@"
	make -C "firmware" DEVICE=atmega32u4 clean
	make -C "firmware" DEVICE=atmega32u4 all
	cp "firmware/build/atmega32u4/main.hex" "$@/thelio-io.hex"
	touch "$@"

output/hardware: FORCE
	rm -rf "$@"
	mkdir -p "$@"
	make -C "hardware" clean
	make -C "hardware" all
	cp "hardware/"*-bom.csv "hardware/"*-pos.csv "hardware/build/"* "$@"

FORCE:
