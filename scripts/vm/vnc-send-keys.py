#!/usr/bin/env python3
"""Minimal VNC client for key injection into the wardrobe Windows build VM.

Connects to the VM's VNC server (default 127.0.0.1:5901), performs the RFB
handshake (no auth), and sends KeyEvent messages. Local Unix socket-class
latency (~1ms), which the QMP input-send-event path could not achieve for the
Windows cdboot "press any key" prompt.

Usage:
  vnc-send-keys.py [--host H] [--port P] hold <keysym-name>   # press and keep held
  vnc-send-keys.py [--host H] [--port P] tap <keysym-name> [count]
  vnc-send-keys.py [--host H] [--port P] type <text>
"""
import socket, sys, time

KEYSYMS = {
    'spc': 0x0020, 'enter': 0xFF0D, 'esc': 0xFF1B, 'tab': 0xFF09,
    'bs': 0xFF08, 'del': 0xFFFF, 'up': 0xFF52, 'down': 0xFF54,
    'left': 0xFF51, 'right': 0xFF53, 'f8': 0xFFC5, 'f10': 0xFFBD,
}

def recv_exact(s, n):
    buf = b''
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if not chunk:
            raise SystemExit('connection closed during handshake')
        buf += chunk
    return buf

class _KeyConn:
    def __init__(self, sock, w, h):
        self.sock, self.w, self.h = sock, w, h
    def sendall(self, b):
        self.sock.sendall(b)
    def setblocking(self, v):
        self.sock.setblocking(v)
    def recv(self, n):
        return self.sock.recv(n)

def connect(host, port):
    s = socket.create_connection((host, port), timeout=5)
    s.settimeout(5)
    banner = recv_exact(s, 12)
    s.sendall(b'RFB 003.008\n')            # use 3.8
    nsec = recv_exact(s, 1)[0]
    types = recv_exact(s, nsec)
    if 1 not in types:                      # None auth
        raise SystemExit(f'no None auth offered: {list(types)}')
    s.sendall(b'\x01')
    recv_exact(s, 4)                        # security result
    s.sendall(b'\x01')                      # client init: shared
    si = recv_exact(s, 20)                  # ServerInit: width/height/pixelformat
    ln = int.from_bytes(recv_exact(s, 4), 'big')
    if ln: recv_exact(s, ln)                # server name
    # Be a well-formed client so qemu keeps the connection: SetPixelFormat (20B)
    # + SetEncodings (8B, raw), matching the server's own pixel format, plus a
    # periodic FramebufferUpdateRequest while typing (qemu drops update-less
    # clients after a couple of seconds).
    w = int.from_bytes(si[0:2], 'big'); h = int.from_bytes(si[2:4], 'big')
    s.sendall(bytes([0, 0, 0, 0, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]))
    s.sendall(bytes([2, 0]) + (1).to_bytes(2, 'big') + (0).to_bytes(4, 'big'))  # raw
    s = _KeyConn(s, w, h)
    return s

def key_event(s, keysym, down):
    try:
        s.sendall(bytes([3, 1]) + (0).to_bytes(2, 'big') + (0).to_bytes(2, 'big')
                  + s.w.to_bytes(2, 'big') + s.h.to_bytes(2, 'big'))
    except AttributeError:
        pass
    s.sendall(bytes([4, 1 if down else 0]) + keysym.to_bytes(4, 'big'))
    # drain any framebuffer-update bytes the server queued for us
    s.setblocking(False)
    try:
        while s.recv(65536):
            pass
    except (BlockingIOError, InterruptedError):
        pass
    s.setblocking(True)

def keysym_for(ch):
    if ch in KEYSYMS: return KEYSYMS[ch]
    o = ord(ch)
    if 0x20 <= o <= 0x7E: return o
    if ch == '\n': return KEYSYMS['enter']
    raise SystemExit(f'no keysym for {ch!r}')

def main():
    args = sys.argv[1:]
    host, port = '127.0.0.1', 5901
    while args and args[0] == '--host':
        host = args[1]; args = args[2:]
    while args and args[0] == '--port':
        port = int(args[1]); args = args[2:]
    if not args:
        raise SystemExit(__doc__)
    cmd = args[0]
    s = connect(host, port)
    if cmd == 'hold':
        key_event(s, KEYSYMS[args[1]], True); time.sleep(0.1)
        print(f'holding {args[1]} (connection stays open {float(args[2]) if len(args)>2 else 3}s)')
        time.sleep(float(args[2]) if len(args) > 2 else 3)
        # hold means keep pressed; we close socket keeping state (server keeps last state)
    elif cmd == 'tap':
        k = KEYSYMS[args[1]]
        n = int(args[2]) if len(args) > 2 else 1
        for _ in range(n):
            key_event(s, k, True); time.sleep(0.05)
            key_event(s, k, False); time.sleep(0.15)
        print(f'tapped {args[1]} x{n}')
    elif cmd == 'type':
        for ch in args[1]:
            k = keysym_for(ch)
            key_event(s, k, True); time.sleep(0.03)
            key_event(s, k, False); time.sleep(0.06)
        key_event(s, KEYSYMS['enter'], True); time.sleep(0.05)
        key_event(s, KEYSYMS['enter'], False)
        print(f'typed {args[1]!r}')
    s.close()

if __name__ == '__main__':
    main()
