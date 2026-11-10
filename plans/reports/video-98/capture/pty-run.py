# pty-run.py LOG CMD…: run an interactive program in its own pseudo-terminal with no window (so no focus changes reach
# it), never typing into it; its screen goes to LOG. For a Claude session whose permission request must stay open.
import os, pty, sys, select, signal, time, fcntl, termios, struct
log = open(sys.argv[1], 'wb')
pid, fd = pty.fork()
if pid == 0:
    os.execvp(sys.argv[2], sys.argv[2:])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
end = time.time() + 3000
while time.time() < end:
    r, _, _ = select.select([fd], [], [], 1)
    if r:
        try: data = os.read(fd, 65536)
        except OSError: break
        if not data: break
        log.write(data); log.flush()
    if os.path.exists(sys.argv[1] + '.in'):
        os.write(fd, open(sys.argv[1] + '.in', 'rb').read()); os.remove(sys.argv[1] + '.in')
    if os.waitpid(pid, os.WNOHANG)[0]: break
os.kill(pid, signal.SIGTERM) if not os.waitpid(pid, os.WNOHANG)[0] else None
