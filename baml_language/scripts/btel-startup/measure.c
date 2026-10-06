#include <errno.h>
#include <libproc.h>
#include <poll.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

extern char **environ;
static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec / 1e9;
}
int main(int argc, char **argv) {
    if (argc < 2) return 64;
    int fds[2];
    if (pipe(fds)) return 65;
    posix_spawn_file_actions_t actions;
    posix_spawn_file_actions_init(&actions);
    posix_spawn_file_actions_adddup2(&actions, fds[1], STDOUT_FILENO);
    posix_spawn_file_actions_addclose(&actions, fds[0]);
    posix_spawn_file_actions_addclose(&actions, fds[1]);
    pid_t pid;
    double start = now(), first = -1;
    int error = posix_spawn(&pid, argv[1], &actions, NULL, &argv[1], environ);
    posix_spawn_file_actions_destroy(&actions);
    close(fds[1]);
    if (error) { errno = error; perror("posix_spawn"); return 66; }
    char buf[65536];
    size_t bytes = 0;
    ssize_t n;
    while ((n = read(fds[0], buf, sizeof(buf))) != 0) {
        if (n < 0) { if (errno == EINTR) continue; break; }
        if (first < 0) first = now();
        bytes += n;
    }
    close(fds[0]);
    struct rusage_info_v4 detailed = {0};
    int detailed_ok = proc_pid_rusage(pid, RUSAGE_INFO_V4, (rusage_info_t *)&detailed) == 0;
    struct rusage usage;
    int status;
    while (wait4(pid, &status, 0, &usage) < 0 && errno == EINTR) {}
    double end = now();
    int code = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
    printf("{\"wall_ms\":%.6f,\"first_output_ms\":%.6f,\"user_ms\":%.6f,\"system_ms\":%.6f,\"peak_rss_bytes\":%ld,\"peak_footprint_bytes\":%lld,\"instructions\":%lld,\"cycles\":%lld,\"disk_read_bytes\":%lld,\"disk_write_bytes\":%lld,\"minor_faults\":%ld,\"major_faults\":%ld,\"voluntary_switches\":%ld,\"involuntary_switches\":%ld,\"stdout_bytes\":%zu,\"exit\":%d}\n",
        (end-start)*1e3, first < 0 ? -1 : (first-start)*1e3,
        (usage.ru_utime.tv_sec + usage.ru_utime.tv_usec/1e6)*1e3,
        (usage.ru_stime.tv_sec + usage.ru_stime.tv_usec/1e6)*1e3,
        usage.ru_maxrss,
        detailed_ok ? (long long)detailed.ri_lifetime_max_phys_footprint : -1,
        detailed_ok ? (long long)detailed.ri_instructions : -1,
        detailed_ok ? (long long)detailed.ri_cycles : -1,
        detailed_ok ? (long long)detailed.ri_diskio_bytesread : -1,
        detailed_ok ? (long long)detailed.ri_diskio_byteswritten : -1,
        usage.ru_minflt, usage.ru_majflt,
        usage.ru_nvcsw, usage.ru_nivcsw, bytes, code);
    return 0;
}
