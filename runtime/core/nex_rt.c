#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

static void nex_rt_write_utf8(unsigned int codepoint) {
    unsigned char buffer[4];
    size_t length = 0;

    if (codepoint <= 0x7F) {
        buffer[0] = (unsigned char)codepoint;
        length = 1;
    } else if (codepoint <= 0x7FF) {
        buffer[0] = (unsigned char)(0xC0 | ((codepoint >> 6) & 0x1F));
        buffer[1] = (unsigned char)(0x80 | (codepoint & 0x3F));
        length = 2;
    } else if (codepoint <= 0xFFFF) {
        buffer[0] = (unsigned char)(0xE0 | ((codepoint >> 12) & 0x0F));
        buffer[1] = (unsigned char)(0x80 | ((codepoint >> 6) & 0x3F));
        buffer[2] = (unsigned char)(0x80 | (codepoint & 0x3F));
        length = 3;
    } else {
        buffer[0] = (unsigned char)(0xF0 | ((codepoint >> 18) & 0x07));
        buffer[1] = (unsigned char)(0x80 | ((codepoint >> 12) & 0x3F));
        buffer[2] = (unsigned char)(0x80 | ((codepoint >> 6) & 0x3F));
        buffer[3] = (unsigned char)(0x80 | (codepoint & 0x3F));
        length = 4;
    }

    fwrite(buffer, 1, length, stdout);
}

void nex_rt_print_str(const char *value) {
    if (value != NULL) {
        fputs(value, stdout);
    }
}

void nex_rt_println_str(const char *value) {
    nex_rt_print_str(value);
    fputc('\n', stdout);
}

void nex_rt_print_i64(long long value) {
    printf("%lld", value);
}

void nex_rt_println_i64(long long value) {
    nex_rt_print_i64(value);
    fputc('\n', stdout);
}

void nex_rt_print_u64(unsigned long long value) {
    printf("%llu", value);
}

void nex_rt_println_u64(unsigned long long value) {
    nex_rt_print_u64(value);
    fputc('\n', stdout);
}

void nex_rt_print_f64(double value) {
    printf("%.15g", value);
}

void nex_rt_println_f64(double value) {
    nex_rt_print_f64(value);
    fputc('\n', stdout);
}

void nex_rt_print_bool(int value) {
    fputs(value ? "true" : "false", stdout);
}

void nex_rt_println_bool(int value) {
    nex_rt_print_bool(value);
    fputc('\n', stdout);
}

void nex_rt_print_char(int value) {
    if (value < 0) {
        fputc('?', stdout);
        return;
    }
    nex_rt_write_utf8((unsigned int)value);
}

void nex_rt_println_char(int value) {
    nex_rt_print_char(value);
    fputc('\n', stdout);
}

void nex_rt_panic(const char *message) {
    if (message != NULL) {
        fprintf(stderr, "NEX runtime error: %s\n", message);
    } else {
        fprintf(stderr, "NEX runtime error\n");
    }
    fflush(stderr);
    exit(1);
}
