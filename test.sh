#!/bin/bash

expected_drives=4
expected_speed=512
expected_devices=1
expected_pwm=127
expected_rpm=600

function fail {
	echo -e "\x1B[1;31mFAIL: $@\x1B[0m"
	exit 1
}

test="tmp/$(date "+%Y-%m-%d_%H:%M:%S")"
rm -rf tmp
mkdir -p "$test"

sudo avrdude -p atmega32u4 -c usbasp -U flash:w:output/firmware/thelio-io.hex:i || fail "failed to flash"
sudo avrdude -p atmega32u4 -c usbasp -U efuse:w:0xCB:m -U hfuse:w:0xD8:m -U lfuse:w:0xFF:m || fail "failed to set fuses"

drives=(/dev/disk/by-path/pci-????:??:??.?-ata-?)
echo "drives: ${#drives[@]}"
if [ "${#drives[@]}" != "$expected_drives" ]
then
	fail "expected $expected_drives drives but found ${#drives[@]} drives"
fi

pids=()
names=()
for drive in "${drives[@]}"
do
	name="$(basename "$drive")"
	sudo hdparm -t "$drive" > "$test/$name" &
	pids+=("$!")
	names+=("$name")
done

for pid in "${pids[@]}"
do
	wait "$pid" || fail "failed to test disk performance"
done

for name in "${names[@]}"
do
	speed="$(grep "Timing buffered disk reads:" "$test/$name" | cut -d '=' -f 2 | cut -d ' ' -f 2)"
	echo "$name: $speed"
	if [ "$(echo "$speed<$expected_speed" | bc -l)" == "1" ]
	then
		fail "expected $expected_speed speed but found $speed speed"
	fi
done

devices=(/sys/bus/usb/drivers/system76-io/?-?:?.1)
echo "devices: ${#devices[@]}"
if [ "${#devices[@]}" != "$expected_devices" ]
then
	fail "expected $expected_devices devices but found ${#devices[@]} devices"
fi

for device in "${devices[@]}"
do
	fan=2

	label="$(cat "$device"/hwmon/hwmon*/fan"$fan"_label)"
	pwm="$(cat "$device"/hwmon/hwmon*/pwm"$fan")"
	rpm="$(cat "$device"/hwmon/hwmon*/fan"$fan"_input)"

	echo "$label: $pwm PWM, $rpm RPM"

	if [ "$pwm" != "$expected_pwm" ]
	then
		fail "expected $expected_pwm pwm but found $pwm pwm"
	fi

	if [ "$(echo "$rpm<$expected_rpm" | bc -l)" == "1" ]
	then
		fail "expected $expected_rpm rpm but found $rpm rpm"
	fi
done

echo -e "\x1B[1;32mPASS\x1B[0m"
exit 0


