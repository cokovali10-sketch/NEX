#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>

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

int nex_rt_str_eq(const char *left, const char *right) {
    if (left == NULL || right == NULL) {
        return left == right;
    }
    return strcmp(left, right) == 0;
}

int nex_rt_str_cmp(const char *left, const char *right) {
    if (left == NULL || right == NULL) {
        if (left == right) {
            return 0;
        }
        return left == NULL ? -1 : 1;
    }
    return strcmp(left, right);
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

long long nex_rt_len(const char *value) {
    if (value == NULL) {
        return 0;
    }
    return (long long)strlen(value);
}

long long nex_rt_abs(long long value) {
    return value < 0 ? -value : value;
}


long long nex_rt_min_i64(long long a, long long b) { return a < b ? a : b; }
long long nex_rt_max_i64(long long a, long long b) { return a > b ? a : b; }
double nex_rt_min_f64(double a, double b) { return a < b ? a : b; }
double nex_rt_max_f64(double a, double b) { return a > b ? a : b; }
double nex_rt_sqrt(double value) { return sqrt(value); }
double nex_rt_pow(double base, double exponent) { return pow(base, exponent); }

