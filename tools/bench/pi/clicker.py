#!/usr/bin/env python3
"""clicker: a Raspberry Pi as the mouse for click-to-photon runs with a camera.

    sudo tools/bench/pi/clicker.py setup         make the USB-C port a USB mouse (once per boot)
    sudo tools/bench/pi/clicker.py run N OUT.csv click N times

tools/bench/pi/install-gadget does the setup at every boot instead, and
lets one user run `run` without sudo.

The Pi plugs into the client by its USB-C port (power and data on one
cable), so the client sees an ordinary USB mouse. At each click it presses
the left button and lights its green status LED in the same instant; a
240 fps camera films the LED and the client's screen together, and
tools/bench/film-clicks reads, for every click, the frame where the LED lit
and the frame where the screen turned white. Nothing of the Pi's runs on the
client or the host, so every stack is clicked the same way.

Why these choices:

- The USB HID gadget is built through configfs (libcomposite), as a boot
  protocol mouse with three-byte reports; the report goes out at the next
  poll of the host controller (bInterval 1: 1 ms at full speed).
- The LED is the status LED (`/sys/class/leds/ACT`, its trigger set to
  none), written right after the report: the two are microseconds apart,
  far under the camera's 4.2 ms a frame.
- The gap between clicks is random (450-850 ms), so clicks fall at every
  phase of both displays' refresh; the button is held 120 ms.

The USB-C port must be in peripheral mode, from the boot configuration
(then reboot; install-gadget writes both):

    /boot/firmware/config.txt, under [all]:  dtoverlay=dwc2,dr_mode=peripheral
    /boot/firmware/cmdline.txt, after rootwait:  modules-load=dwc2
"""
import os
import random
import sys
import time

GADGET = "/sys/kernel/config/usb_gadget/bench-mouse"
LED = "/sys/class/leds/ACT"
# A boot protocol mouse: three buttons, relative X and Y (bytes 2 and 3).
REPORT_DESCRIPTOR = bytes([
    0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00,
    0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00, 0x25, 0x01,
    0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05,
    0x81, 0x01, 0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x15, 0x81,
    0x25, 0x7F, 0x75, 0x08, 0x95, 0x02, 0x81, 0x06, 0xC0, 0xC0,
])


def write(path, value, mode="w"):
    with open(path, mode) as f:
        f.write(value)


def setup():
    os.system("modprobe libcomposite")
    if os.path.isdir(GADGET):
        print("gadget already set up")
        return
    os.makedirs(GADGET)
    write(f"{GADGET}/idVendor", "0x1d6b")  # Linux Foundation
    write(f"{GADGET}/idProduct", "0x0104")  # Multifunction Composite Gadget
    write(f"{GADGET}/bcdUSB", "0x0200")
    os.makedirs(f"{GADGET}/strings/0x409")
    write(f"{GADGET}/strings/0x409/manufacturer", "bench")
    write(f"{GADGET}/strings/0x409/product", "bench clicker")
    write(f"{GADGET}/strings/0x409/serialnumber", "0001")
    os.makedirs(f"{GADGET}/configs/c.1/strings/0x409")
    write(f"{GADGET}/configs/c.1/strings/0x409/configuration", "mouse")
    write(f"{GADGET}/configs/c.1/MaxPower", "100")
    fn = f"{GADGET}/functions/hid.usb0"
    os.makedirs(fn)
    write(f"{fn}/protocol", "2")  # mouse
    write(f"{fn}/subclass", "1")  # boot interface
    write(f"{fn}/report_length", "3")
    write(f"{fn}/report_desc", REPORT_DESCRIPTOR, "wb")
    os.symlink(fn, f"{GADGET}/configs/c.1/hid.usb0")
    udc = os.listdir("/sys/class/udc")
    if not udc:
        sys.exit("no USB device controller: is dtoverlay=dwc2,dr_mode=peripheral set, and the Pi rebooted?")
    write(f"{GADGET}/UDC", udc[0])
    print(f"a USB mouse on {udc[0]}")


def run(n, out):
    write(f"{LED}/trigger", "none")
    write(f"{LED}/brightness", "0")
    hid = os.open("/dev/hidg0", os.O_WRONLY)
    rows = ["click,down_ns,up_ns"]
    try:
        time.sleep(2)
        for i in range(n):
            time.sleep(random.uniform(0.45, 0.85))
            os.write(hid, b"\x01\x00\x00")
            write(f"{LED}/brightness", "1")
            down = time.monotonic_ns()
            time.sleep(0.12)
            os.write(hid, b"\x00\x00\x00")
            write(f"{LED}/brightness", "0")
            rows.append(f"{i},{down},{time.monotonic_ns()}")
    finally:
        os.close(hid)
        write(f"{LED}/trigger", "mmc0")
    with open(out, "w") as f:
        f.write("\n".join(rows) + "\n")
    print(f"{n} clicks")


if __name__ == "__main__":
    if len(sys.argv) >= 2 and sys.argv[1] == "setup":
        setup()
    elif len(sys.argv) == 4 and sys.argv[1] == "run":
        run(int(sys.argv[2]), sys.argv[3])
    else:
        sys.exit(__doc__)
