#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--version") == 0) {
        puts("baml-cli 0.11.0");
        return 0;
    }
    if (argc > 1 && strcmp(argv[1], "stream") == 0) {
        char bytes[65536];
        memset(bytes, 'x', sizeof(bytes));
        for (int i = 0; i < 512; i++) if (write(1, bytes, sizeof(bytes)) < 0) return 1;
        return 0;
    }
    if (argc > 1 && strcmp(argv[1], "hold") == 0) {
        size_t size = (argc > 2 ? strtol(argv[2], NULL, 10) : 0) * 1024 * 1024;
        volatile char *bytes = malloc(size);
        for (size_t i = 0; i < size; i += 4096) bytes[i] = 1;
        printf("ready %d\n", getpid());
        fflush(stdout);
        sleep(2);
        free((void *)bytes);
        return 0;
    }
    puts("ready");
    return 0;
}
