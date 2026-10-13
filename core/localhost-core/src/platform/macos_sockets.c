// The TCP ports a process listens on, through libproc's socket info.
//
// In C rather than Rust because `struct socket_fdinfo` is a deep tree of
// unions that the libc crate doesn't describe; compiling against the SDK's
// <sys/proc_info.h> keeps its layout right.

#include <libproc.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/proc_info.h>

// Writes up to `capacity` listening ports of `pid` (unsorted, maybe repeated)
// to `ports` and returns how many there are, or 0 if `pid` can't be inspected.
int localhost_listening_ports(pid_t pid, uint16_t *ports, int capacity) {
    int size = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, NULL, 0);
    if (size <= 0) return 0;
    struct proc_fdinfo *fds = malloc((size_t)size);
    if (fds == NULL) return 0;
    int filled = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, fds, size);
    int count = 0;
    for (int i = 0; filled > 0 && i < filled / (int)sizeof(struct proc_fdinfo) && count < capacity; i++) {
        if (fds[i].proc_fdtype != PROX_FDTYPE_SOCKET) continue;
        struct socket_fdinfo info;
        int infoSize = proc_pidfdinfo(pid, fds[i].proc_fd, PROC_PIDFDSOCKETINFO, &info, sizeof(info));
        if (infoSize != (int)sizeof(info)) continue;
        if (info.psi.soi_kind != SOCKINFO_TCP) continue;
        if (info.psi.soi_proto.pri_tcp.tcpsi_state != TSI_S_LISTEN) continue;
        uint16_t port = ntohs((uint16_t)info.psi.soi_proto.pri_tcp.tcpsi_ini.insi_lport);
        if (port != 0) ports[count++] = port;
    }
    free(fds);
    return count;
}
