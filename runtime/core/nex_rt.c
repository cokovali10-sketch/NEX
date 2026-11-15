#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <ctype.h>

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


long long nex_rt_alloc(long long size) {
    if (size <= 0) {
        return 0;
    }
    return (long long)malloc((size_t)size);
}

void nex_rt_free(long long ptr) {
    free((void *)ptr);
}


long long nex_rt_peek_i64(long long ptr) { return *(long long *)ptr; }
void nex_rt_poke_i64(long long ptr, long long value) { *(long long *)ptr = value; }

char *nex_rt_str_concat(const char *a, const char *b) {
    size_t la = a ? strlen(a) : 0;
    size_t lb = b ? strlen(b) : 0;
    char *result = (char *)malloc(la + lb + 1);
    if (result == NULL) {
        return NULL;
    }
    if (a) {
        memcpy(result, a, la);
    }
    if (b) {
        memcpy(result + la, b, lb);
    }
    result[la + lb] = '\0';
    return result;
}

char *nex_rt_i64_to_string(long long value) {
    char buffer[32];
    int length = snprintf(buffer, sizeof(buffer), "%lld", value);
    if (length < 0) {
        return NULL;
    }
    char *result = (char *)malloc((size_t)length + 1);
    if (result == NULL) {
        return NULL;
    }
    memcpy(result, buffer, (size_t)length + 1);
    return result;
}

int nex_rt_str_contains(const char *haystack, const char *needle) {
    if (haystack == NULL || needle == NULL) {
        return 0;
    }
    return strstr(haystack, needle) != NULL;
}

long long nex_rt_str_index_of(const char *haystack, const char *needle) {
    if (haystack == NULL || needle == NULL) {
        return -1;
    }
    const char *found = strstr(haystack, needle);
    if (found == NULL) {
        return -1;
    }
    return (long long)(found - haystack);
}

char *nex_rt_str_substring(const char *value, long long start, long long end) {
    if (value == NULL) {
        return NULL;
    }
    long long length = (long long)strlen(value);
    if (start < 0) {
        start = 0;
    }
    if (end > length) {
        end = length;
    }
    if (end < start) {
        end = start;
    }
    long long count = end - start;
    char *result = (char *)malloc((size_t)count + 1);
    if (result == NULL) {
        return NULL;
    }
    memcpy(result, value + start, (size_t)count);
    result[count] = '\0';
    return result;
}

char *nex_rt_str_upper(const char *value) {
    if (value == NULL) {
        return NULL;
    }
    size_t length = strlen(value);
    char *result = (char *)malloc(length + 1);
    if (result == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < length; i++) {
        result[i] = (char)toupper((unsigned char)value[i]);
    }
    result[length] = '\0';
    return result;
}

char *nex_rt_str_lower(const char *value) {
    if (value == NULL) {
        return NULL;
    }
    size_t length = strlen(value);
    char *result = (char *)malloc(length + 1);
    if (result == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < length; i++) {
        result[i] = (char)tolower((unsigned char)value[i]);
    }
    result[length] = '\0';
    return result;
}

char *nex_rt_str_trim(const char *value) {
    if (value == NULL) {
        return NULL;
    }
    const char *start = value;
    while (*start == ' ' || *start == '\t' || *start == '\r' || *start == '\n') {
        start++;
    }
    const char *end = start + strlen(start);
    while (end > start) {
        char last = end[-1];
        if (last != ' ' && last != '\t' && last != '\r' && last != '\n') {
            break;
        }
        end--;
    }
    size_t count = (size_t)(end - start);
    char *result = (char *)malloc(count + 1);
    if (result == NULL) {
        return NULL;
    }
    memcpy(result, start, count);
    result[count] = '\0';
    return result;
}

int nex_rt_str_starts_with(const char *value, const char *prefix) {
    if (value == NULL || prefix == NULL) {
        return 0;
    }
    size_t prefix_length = strlen(prefix);
    return strncmp(value, prefix, prefix_length) == 0;
}

int nex_rt_str_ends_with(const char *value, const char *suffix) {
    if (value == NULL || suffix == NULL) {
        return 0;
    }
    size_t value_length = strlen(value);
    size_t suffix_length = strlen(suffix);
    if (suffix_length > value_length) {
        return 0;
    }
    return strcmp(value + (value_length - suffix_length), suffix) == 0;
}

char *nex_rt_str_repeat(const char *value, long long count) {
    if (value == NULL) {
        return NULL;
    }
    if (count <= 0) {
        char *empty = (char *)malloc(1);
        if (empty != NULL) {
            empty[0] = '\0';
        }
        return empty;
    }
    size_t value_length = strlen(value);
    size_t total = value_length * (size_t)count;
    char *result = (char *)malloc(total + 1);
    if (result == NULL) {
        return NULL;
    }
    for (long long i = 0; i < count; i++) {
        memcpy(result + (size_t)i * value_length, value, value_length);
    }
    result[total] = '\0';
    return result;
}

char *nex_rt_str_replace(const char *value, const char *from, const char *to) {
    if (value == NULL || from == NULL || to == NULL) {
        return NULL;
    }
    size_t from_length = strlen(from);
    size_t to_length = strlen(to);
    if (from_length == 0) {
        // An empty pattern would loop forever; return a copy instead.
        size_t length = strlen(value);
        char *copy = (char *)malloc(length + 1);
        if (copy != NULL) {
            memcpy(copy, value, length + 1);
        }
        return copy;
    }

    size_t value_length = strlen(value);
    size_t matches = 0;
    const char *cursor = value;
    while ((cursor = strstr(cursor, from)) != NULL) {
        matches++;
        cursor += from_length;
    }
    if (matches == 0) {
        char *copy = (char *)malloc(value_length + 1);
        if (copy != NULL) {
            memcpy(copy, value, value_length + 1);
        }
        return copy;
    }

    size_t result_length = value_length + matches * to_length;
    char *result = (char *)malloc(result_length + 1);
    if (result == NULL) {
        return NULL;
    }

    size_t written = 0;
    const char *read = value;
    const char *found;
    while ((found = strstr(read, from)) != NULL) {
        size_t chunk = (size_t)(found - read);
        memcpy(result + written, read, chunk);
        written += chunk;
        memcpy(result + written, to, to_length);
        written += to_length;
        read = found + from_length;
    }
    size_t tail = strlen(read);
    memcpy(result + written, read, tail);
    written += tail;
    result[written] = '\0';
    return result;
}