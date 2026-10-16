#define _GNU_SOURCE
#include <errno.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>
#include <time.h>

/* A small, freshly exec'd parent prevents the Python launcher's inherited
   pre-exec RSS from contaminating wait4's high-water mark. */
int main(int argc, char **argv) {
    if (argc < 6) return 125;
    if (chdir(argv[3])) { perror("chdir"); return 125; }
    struct timespec started, ended;
    clock_gettime(CLOCK_MONOTONIC, &started);
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 125; }
    if (!pid) {
        if (strcmp(argv[4], "inherit")) {
            cpu_set_t mask;
            CPU_ZERO(&mask);
            for (char *s = strtok(argv[4], ","); s; s = strtok(NULL, ",")) {
                char *end;
                long cpu = strtol(s, &end, 10);
                if (*end || cpu < 0 || cpu >= CPU_SETSIZE) _exit(125);
                CPU_SET(cpu, &mask);
            }
            if (sched_setaffinity(0, sizeof(mask), &mask)) { perror("affinity"); _exit(125); }
        }
        execv(argv[5], argv + 5);
        perror("execv");
        _exit(127);
    }
    FILE *pid_file = fopen(argv[1], "w");
    if (!pid_file) { kill(pid, SIGKILL); waitpid(pid, NULL, 0); return 125; }
    fprintf(pid_file, "%d\n", pid);
    fclose(pid_file);
    struct rusage usage;
    int status;
    while (wait4(pid, &status, 0, &usage) < 0) {
        if (errno != EINTR) { perror("wait4"); return 125; }
    }
    clock_gettime(CLOCK_MONOTONIC, &ended);
    long long wall_ns = (ended.tv_sec-started.tv_sec)*1000000000LL + ended.tv_nsec-started.tv_nsec;
    FILE *output = fopen(argv[2], "w");
    if (!output) return 125;
    fprintf(output, "{\"user_cpu_s\":%.6f,\"system_cpu_s\":%.6f,\"peak_rss_bytes\":%ld,\"wall_ns\":%lld}\n",
            usage.ru_utime.tv_sec + usage.ru_utime.tv_usec / 1e6,
            usage.ru_stime.tv_sec + usage.ru_stime.tv_usec / 1e6,
            usage.ru_maxrss * 1024L, wall_ns);
    fclose(output);
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}
