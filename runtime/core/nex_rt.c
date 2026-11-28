#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <limits.h>
#include <string.h>
#include <math.h>
#include <ctype.h>
#include <time.h>
#include <stdatomic.h>

/* Process-wide state captured from the generated `main`. */
static int nex_rt_argc = 0;
static char **nex_rt_argv = NULL;

/* Serializes whole log lines so worker threads cannot interleave them. */
static atomic_flag nex_rt_log_lock = ATOMIC_FLAG_INIT;

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

/* Unicode-aware uppercase: handles Cyrillic and basic Latin */
char *nex_rt_utf8_upper(const char *value) {
    if (value == NULL) return NULL;
    size_t byte_len = strlen(value);
    char *result = (char *)malloc(byte_len + 1);
    if (result == NULL) return NULL;
    
    size_t src = 0, dst = 0;
    while (value[src] != '\0') {
        unsigned char b = (unsigned char)value[src];
        if (b < 0x80) {
            /* ASCII */
            result[dst++] = (char)toupper(b);
            src++;
        } else if ((b & 0xE0) == 0xC0) {
            /* 2-byte UTF-8 */
            unsigned char b2 = (unsigned char)value[src + 1];
            long long cp = ((b & 0x1F) << 6) | (b2 & 0x3F);
            /* Cyrillic lowercase to uppercase */
            if (cp >= 0x0430 && cp <= 0x044F) cp -= 0x20;
            else if (cp == 0x0451) cp = 0x0401; /* ё -> Ё */
            /* Encode back */
            result[dst++] = (char)(0xC0 | ((cp >> 6) & 0x1F));
            result[dst++] = (char)(0x80 | (cp & 0x3F));
            src += 2;
        } else if ((b & 0xF0) == 0xE0) {
            /* 3-byte UTF-8 - pass through */
            result[dst++] = value[src++];
            result[dst++] = value[src++];
            result[dst++] = value[src++];
        } else {
            /* 4-byte or invalid - pass through */
            result[dst++] = value[src++];
        }
    }
    result[dst] = '\0';
    return result;
}

/* Unicode-aware lowercase: handles Cyrillic and basic Latin */
char *nex_rt_utf8_lower(const char *value) {
    if (value == NULL) return NULL;
    size_t byte_len = strlen(value);
    char *result = (char *)malloc(byte_len + 1);
    if (result == NULL) return NULL;
    
    size_t src = 0, dst = 0;
    while (value[src] != '\0') {
        unsigned char b = (unsigned char)value[src];
        if (b < 0x80) {
            /* ASCII */
            result[dst++] = (char)tolower(b);
            src++;
        } else if ((b & 0xE0) == 0xC0) {
            /* 2-byte UTF-8 */
            unsigned char b2 = (unsigned char)value[src + 1];
            long long cp = ((b & 0x1F) << 6) | (b2 & 0x3F);
            /* Cyrillic uppercase to lowercase */
            if (cp >= 0x0410 && cp <= 0x042F) cp += 0x20;
            else if (cp == 0x0401) cp = 0x0451; /* Ё -> ё */
            /* Encode back */
            result[dst++] = (char)(0xC0 | ((cp >> 6) & 0x1F));
            result[dst++] = (char)(0x80 | (cp & 0x3F));
            src += 2;
        } else if ((b & 0xF0) == 0xE0) {
            /* 3-byte UTF-8 - pass through */
            result[dst++] = value[src++];
            result[dst++] = value[src++];
            result[dst++] = value[src++];
        } else {
            /* 4-byte or invalid - pass through */
            result[dst++] = value[src++];
        }
    }
    result[dst] = '\0';
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
    /* Guard against `value_length * count` overflowing size_t: without this
       check the multiplication wraps to a small number, `malloc` succeeds, and
       the memcpy loop below writes far past the end of the allocation. */
    if (value_length != 0 && (size_t)count > (SIZE_MAX - 1) / value_length) {
        return NULL;
    }
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

double nex_rt_abs_f64(double value) {
    return value < 0 ? -value : value;
}

char *nex_rt_f64_to_string(double value) {
    char buffer[64];
    int length = snprintf(buffer, sizeof(buffer), "%g", value);
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

char *nex_rt_bool_to_string(int value) {
    const char *text = value ? "true" : "false";
    size_t length = strlen(text);
    char *result = (char *)malloc(length + 1);
    if (result == NULL) {
        return NULL;
    }
    memcpy(result, text, length + 1);
    return result;
}

char *nex_rt_str_reverse(const char *value) {
    if (value == NULL) {
        return NULL;
    }
    size_t length = strlen(value);
    char *result = (char *)malloc(length + 1);
    if (result == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < length; i++) {
        result[i] = value[length - 1 - i];
    }
    result[length] = '\0';
    return result;
}

long long nex_rt_parse_i64(const char *value) {
    if (value == NULL || *value == '\0') {
        return 0;
    }
    char *end = NULL;
    long long parsed = strtoll(value, &end, 10);
    if (end == value) {
        return 0;
    }
    return parsed;
}

char *nex_rt_read_file(const char *path) {
    if (path == NULL) {
        return NULL;
    }
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return NULL;
    }
    if (fseek(file, 0, SEEK_END) != 0) {
        fclose(file);
        return NULL;
    }
    long length = ftell(file);
    if (length < 0) {
        fclose(file);
        return NULL;
    }
    rewind(file);
    char *buffer = (char *)malloc((size_t)length + 1);
    if (buffer == NULL) {
        fclose(file);
        return NULL;
    }
    size_t read = fread(buffer, 1, (size_t)length, file);
    fclose(file);
    buffer[read] = '\0';
    return buffer;
}

int nex_rt_write_file(const char *path, const char *content) {
    if (path == NULL || content == NULL) {
        return 0;
    }
    FILE *file = fopen(path, "wb");
    if (file == NULL) {
        return 0;
    }
    size_t length = strlen(content);
    size_t written = fwrite(content, 1, length, file);
    fclose(file);
    return written == length;
}

int nex_rt_file_exists(const char *path) {
    if (path == NULL) {
        return 0;
    }
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return 0;
    }
    fclose(file);
    return 1;
}

long long nex_rt_file_size(const char *path) {
    if (path == NULL) {
        return -1;
    }
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return -1;
    }
    if (fseek(file, 0, SEEK_END) != 0) {
        fclose(file);
        return -1;
    }
    long length = ftell(file);
    fclose(file);
    return (long long)length;
}

/* ------------------------------------------------------------------------- */
/* Minimal TCP sockets: blocking, string-oriented. No async runtime yet.      */
/* ------------------------------------------------------------------------- */

#ifdef _WIN32
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0601
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#pragma comment(lib, "ws2_32.lib")
typedef SOCKET nex_socket;
#define NEX_INVALID_SOCKET INVALID_SOCKET
#define NEX_CLOSE_SOCKET(s) closesocket(s)
static int nex_net_startup(void) {
    WSADATA data;
    return WSAStartup(MAKEWORD(2, 2), &data) == 0;
}
static void nex_net_shutdown(void) {
    WSACleanup();
}
#else
#include <sys/socket.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <unistd.h>
typedef int nex_socket;
#define NEX_INVALID_SOCKET (-1)
#define NEX_CLOSE_SOCKET(s) close(s)
static int nex_net_startup(void) {
    return 1;
}
static void nex_net_shutdown(void) {
}
#endif

static int nex_net_ready = 0;

static int nex_net_ensure(void) {
    if (nex_net_ready) {
        return 1;
    }
    if (nex_net_startup()) {
        nex_net_ready = 1;
        return 1;
    }
    return 0;
}

long long nex_rt_tcp_connect(const char *host, long long port) {
    if (host == NULL || port <= 0 || port > 65535) {
        return -1;
    }
    if (!nex_net_ensure()) {
        return -1;
    }

    struct addrinfo hints;
    struct addrinfo *result = NULL;
    char service[16];
    snprintf(service, sizeof(service), "%lld", port);

    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    if (getaddrinfo(host, service, &hints, &result) != 0) {
        return -1;
    }

    nex_socket fd = NEX_INVALID_SOCKET;
    for (struct addrinfo *entry = result; entry != NULL; entry = entry->ai_next) {
        fd = socket(entry->ai_family, entry->ai_socktype, entry->ai_protocol);
        if (fd == NEX_INVALID_SOCKET) {
            continue;
        }
        if (connect(fd, entry->ai_addr, (int)entry->ai_addrlen) == 0) {
            break;
        }
        NEX_CLOSE_SOCKET(fd);
        fd = NEX_INVALID_SOCKET;
    }
    freeaddrinfo(result);

    if (fd == NEX_INVALID_SOCKET) {
        return -1;
    }
    return (long long)fd;
}

long long nex_rt_tcp_listen(long long port, long long backlog) {
    if (port <= 0 || port > 65535) {
        return -1;
    }
    if (!nex_net_ensure()) {
        return -1;
    }

    nex_socket server = socket(AF_INET, SOCK_STREAM, 0);
    if (server == NEX_INVALID_SOCKET) {
        return -1;
    }

    int yes = 1;
    setsockopt(server, SOL_SOCKET, SO_REUSEADDR, (const char *)&yes, sizeof(yes));

    struct sockaddr_in address;
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    address.sin_port = htons((unsigned short)port);

    if (bind(server, (struct sockaddr *)&address, sizeof(address)) != 0) {
        NEX_CLOSE_SOCKET(server);
        return -1;
    }
    int queue = backlog > 0 ? (int)backlog : 16;
    if (listen(server, queue) != 0) {
        NEX_CLOSE_SOCKET(server);
        return -1;
    }
    return (long long)server;
}

long long nex_rt_tcp_accept(long long server) {
    if (server < 0 || !nex_net_ensure()) {
        return -1;
    }
    nex_socket client = accept((nex_socket)server, NULL, NULL);
    if (client == NEX_INVALID_SOCKET) {
        return -1;
    }
    return (long long)client;
}

long long nex_rt_tcp_write(long long socket, const char *data, long long length) {
    if (socket < 0 || data == NULL || length < 0) {
        return -1;
    }
    long long sent = 0;
    while (sent < length) {
        int chunk = send((nex_socket)socket, data + sent, (int)(length - sent), 0);
        if (chunk <= 0) {
            return -1;
        }
        sent += chunk;
    }
    return sent;
}

char *nex_rt_tcp_read(long long socket, long long max_bytes) {
    if (socket < 0 || max_bytes <= 0) {
        return NULL;
    }
    long long limit = max_bytes > 262144 ? 262144 : max_bytes;
    char *buffer = (char *)malloc((size_t)limit + 1);
    if (buffer == NULL) {
        return NULL;
    }
    int received = recv((nex_socket)socket, buffer, (int)limit, 0);
    if (received <= 0) {
        free(buffer);
        char *empty = (char *)malloc(1);
        if (empty != NULL) {
            empty[0] = '\0';
        }
        return empty;
    }
    buffer[received] = '\0';
    return buffer;
}

int nex_rt_tcp_close(long long socket) {
    if (socket < 0) {
        return 0;
    }
    NEX_CLOSE_SOCKET((nex_socket)socket);
    return 1;
}

/* ------------------------------------------------------------------------- */
/* Minimal HTTP/1.1 helpers: parse a request head, build a response.          */
/* ------------------------------------------------------------------------- */

static const char *nex_http_skip_spaces(const char *cursor) {
    while (*cursor == ' ' || *cursor == '\t') {
        cursor++;
    }
    return cursor;
}

static char *nex_http_token(const char *start, size_t length) {
    char *token = (char *)malloc(length + 1);
    if (token == NULL) {
        return NULL;
    }
    memcpy(token, start, length);
    token[length] = '\0';
    return token;
}

/* Returns a copy of the request method: "GET", "POST", ... */
char *nex_rt_http_method(const char *request) {
    if (request == NULL) {
        return NULL;
    }
    const char *cursor = nex_http_skip_spaces(request);
    const char *end = cursor;
    while (*end != '\0' && *end != ' ') {
        end++;
    }
    return nex_http_token(cursor, (size_t)(end - cursor));
}

/* Returns the request target, e.g. "/index.html". */
char *nex_rt_http_path(const char *request) {
    if (request == NULL) {
        return NULL;
    }
    const char *method_end = request;
    while (*method_end != '\0' && *method_end != ' ') {
        method_end++;
    }
    if (*method_end == '\0') {
        return NULL;
    }
    const char *path = nex_http_skip_spaces(method_end);
    const char *end = path;
    while (*end != '\0' && *end != ' ' && *end != '\r' && *end != '\n') {
        end++;
    }
    return nex_http_token(path, (size_t)(end - path));
}

/* Returns the HTTP version token, e.g. "HTTP/1.1". */
char *nex_rt_http_version(const char *request) {
    if (request == NULL) {
        return NULL;
    }
    const char *cursor = request;
    for (int field = 0; field < 2; field++) {
        while (*cursor != '\0' && *cursor != ' ') {
            cursor++;
        }
        if (*cursor == '\0') {
            return NULL;
        }
        cursor = nex_http_skip_spaces(cursor);
    }
    const char *end = cursor;
    while (*end != '\0' && *end != ' ' && *end != '\r' && *end != '\n') {
        end++;
    }
    return nex_http_token(cursor, (size_t)(end - cursor));
}

/* Case-insensitive header lookup inside the request head. */
char *nex_rt_http_header(const char *request, const char *name) {
    if (request == NULL || name == NULL) {
        return NULL;
    }
    size_t name_length = strlen(name);
    const char *cursor = request;
    while (*cursor != '\0') {
        const char *line_end = cursor;
        while (*line_end != '\0' && *line_end != '\n') {
            line_end++;
        }
        if (line_end > cursor && cursor[0] != '\r' && cursor[0] != '\n') {
            size_t line_length = (size_t)(line_end - cursor);
            int matches = line_length > name_length;
            for (size_t i = 0; matches && i < name_length; i++) {
                char left = cursor[i];
                char right = name[i];
                if (left >= 'A' && left <= 'Z') {
                    left = (char)(left - 'A' + 'a');
                }
                if (right >= 'A' && right <= 'Z') {
                    right = (char)(right - 'A' + 'a');
                }
                if (left != right) {
                    matches = 0;
                }
            }
            if (matches && cursor[name_length] == ':') {
                const char *value = nex_http_skip_spaces(cursor + name_length + 1);
                const char *value_end = line_end;
                if (value_end > value && value_end[-1] == '\r') {
                    value_end--;
                }
                return nex_http_token(value, (size_t)(value_end - value));
            }
        }
        if (*line_end == '\0') {
            break;
        }
        cursor = line_end + 1;
    }
    return NULL;
}

/* Body = everything after the first empty line. */
char *nex_rt_http_body(const char *request) {
    if (request == NULL) {
        return NULL;
    }
    const char *cursor = request;
    while (*cursor != '\0') {
        if (cursor[0] == '\r' && cursor[1] == '\n' && cursor[2] == '\r' && cursor[3] == '\n') {
            cursor += 4;
            break;
        }
        if (cursor[0] == '\n' && cursor[1] == '\n') {
            cursor += 2;
            break;
        }
        cursor++;
    }
    size_t length = strlen(cursor);
    char *body = (char *)malloc(length + 1);
    if (body == NULL) {
        return NULL;
    }
    memcpy(body, cursor, length + 1);
    return body;
}

/* Builds a complete HTTP/1.1 response with Content-Length and Connection: close. */
char *nex_rt_http_response(const char *status, const char *content_type, const char *body) {
    if (status == NULL) {
        status = "200 OK";
    }
    if (content_type == NULL || *content_type == '\0') {
        content_type = "text/plain; charset=utf-8";
    }
    if (body == NULL) {
        body = "";
    }
    size_t status_length = strlen(status);
    size_t type_length = strlen(content_type);
    size_t body_length = strlen(body);
    size_t digits = 1;
    for (size_t value = body_length; value >= 10; value /= 10) {
        digits++;
    }
    size_t head_length = status_length + type_length + digits + 128;
    char *response = (char *)malloc(head_length + body_length + 1);
    if (response == NULL) {
        return NULL;
    }
    snprintf(
        response,
        head_length + body_length + 1,
        "HTTP/1.1 %s\r\nContent-Type: %s\r\nContent-Length: %zu\r\nConnection: close\r\n\r\n%s",
        status,
        content_type,
        body_length,
        body
    );
    return response;
}

/* ------------------------------------------------------------------------- */
/* Time helpers.                                                            */
/* ------------------------------------------------------------------------- */

long long nex_rt_unix_time(void) {
    return (long long)time(NULL);
}

long long nex_rt_now_ms(void) {
#ifdef _WIN32
    static LARGE_INTEGER frequency = {0};
    if (frequency.QuadPart == 0) {
        QueryPerformanceFrequency(&frequency);
    }
    LARGE_INTEGER counter;
    QueryPerformanceCounter(&counter);
    return (long long)((counter.QuadPart * 1000) / frequency.QuadPart);
#else
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (long long)ts.tv_sec * 1000 + (long long)(ts.tv_nsec / 1000000);
#endif
}

void nex_rt_sleep_ms(long long milliseconds) {
    if (milliseconds <= 0) {
        return;
    }
#ifdef _WIN32
    Sleep((DWORD)milliseconds);
#else
    struct timespec ts;
    ts.tv_sec = (time_t)(milliseconds / 1000);
    ts.tv_nsec = (long)((milliseconds % 1000) * 1000000);
    nanosleep(&ts, NULL);
#endif
}

/* RFC 7231 IMF-fixdate, e.g. "Sun, 15 Nov 2026 14:45:33 GMT". */
char *nex_rt_http_date(void) {
    time_t now = time(NULL);
    struct tm broken_down;
#ifdef _WIN32
    gmtime_s(&broken_down, &now);
#else
    gmtime_r(&now, &broken_down);
#endif
    char buffer[32];
    if (strftime(buffer, sizeof(buffer), "%a, %d %b %Y %H:%M:%S GMT", &broken_down) == 0) {
        return NULL;
    }
    size_t length = strlen(buffer);
    char *result = (char *)malloc(length + 1);
    if (result == NULL) {
        return NULL;
    }
    memcpy(result, buffer, length + 1);
    return result;
}

long long nex_rt_str_index_of_from(const char *haystack, const char *needle, long long start) {
    if (haystack == NULL || needle == NULL) {
        return -1;
    }
    size_t haystack_length = strlen(haystack);
    if (start < 0) {
        return -1;
    }
    if ((size_t)start >= haystack_length) {
        return (*needle == '\0') ? (long long)haystack_length : -1;
    }
    const char *found = strstr(haystack + start, needle);
    if (found == NULL) {
        return -1;
    }
    return (long long)(found - haystack);
}

char *nex_rt_char_at(const char *value, long long index) {
    if (value == NULL || index < 0) {
        return NULL;
    }
    size_t length = strlen(value);
    if ((size_t)index >= length) {
        return NULL;
    }
    char buffer[2];
    buffer[0] = value[index];
    buffer[1] = '\0';
    char *result = (char *)malloc(2);
    if (result == NULL) {
        return NULL;
    }
    result[0] = buffer[0];
    result[1] = '\0';
    return result;
}

/* Helper: count UTF-8 code points in a string */
static size_t nex_rt_utf8_count(const char *str) {
    if (str == NULL) return 0;
    size_t count = 0;
    const unsigned char *p = (const unsigned char *)str;
    while (*p != '\0') {
        /* Determine character length from first byte */
        if (*p < 0x80) p += 1;
        else if ((*p & 0xE0) == 0xC0) p += 2;
        else if ((*p & 0xF0) == 0xE0) p += 3;
        else if ((*p & 0xF8) == 0xF0) p += 4;
        else p += 1; /* invalid, skip */
        count++;
    }
    return count;
}

/* Helper: get byte offset for code point index */
static size_t nex_rt_utf8_byte_offset(const char *str, size_t char_idx) {
    if (str == NULL) return 0;
    size_t count = 0;
    const unsigned char *p = (const unsigned char *)str;
    while (*p != '\0' && count < char_idx) {
        if (*p < 0x80) p += 1;
        else if ((*p & 0xE0) == 0xC0) p += 2;
        else if ((*p & 0xF0) == 0xE0) p += 3;
        else if ((*p & 0xF8) == 0xF0) p += 4;
        else p += 1;
        count++;
    }
    return (size_t)(p - (const unsigned char *)str);
}

/* Helper: get codepoint at byte position */
static long long nex_rt_utf8_codepoint_at(const char *str, size_t byte_idx) {
    if (str == NULL) return -1;
    const unsigned char *p = (const unsigned char *)str + byte_idx;
    if (*p == '\0') return -1;
    unsigned char first = *p;
    if (first < 0x80) {
        return (long long)first;
    } else if ((first & 0xE0) == 0xC0) {
        if (p[1] == '\0') return -1;
        return ((first & 0x1F) << 6) | (p[1] & 0x3F);
    } else if ((first & 0xF0) == 0xE0) {
        if (p[1] == '\0' || p[2] == '\0') return -1;
        return ((first & 0x0F) << 12) | ((p[1] & 0x3F) << 6) | (p[2] & 0x3F);
    } else if ((first & 0xF8) == 0xF0) {
        if (p[1] == '\0' || p[2] == '\0' || p[3] == '\0') return -1;
        return ((first & 0x07) << 18) | ((p[1] & 0x3F) << 12) |
               ((p[2] & 0x3F) << 6) | (p[3] & 0x3F);
    }
    return -1;
}

/* Helper: get byte length of codepoint at byte position */
static size_t nex_rt_utf8_codepoint_len(const char *str, size_t byte_idx) {
    if (str == NULL) return 0;
    const unsigned char *p = (const unsigned char *)str + byte_idx;
    if (*p == '\0') return 0;
    if (*p < 0x80) return 1;
    if ((*p & 0xE0) == 0xC0) return 2;
    if ((*p & 0xF0) == 0xE0) return 3;
    if ((*p & 0xF8) == 0xF0) return 4;
    return 1;
}

/* Unicode-aware string length (code point count) */
long long nex_rt_utf8_len(const char *value) {
    if (value == NULL) return 0;
    return (long long)nex_rt_utf8_count(value);
}

/* Unicode-aware char_at: returns UTF-8 string for the character at codepoint index */
char *nex_rt_utf8_char_at(const char *value, long long index) {
    if (value == NULL || index < 0) return NULL;
    size_t char_count = nex_rt_utf8_count(value);
    if ((size_t)index >= char_count) return NULL;
    
    size_t byte_idx = nex_rt_utf8_byte_offset(value, (size_t)index);
    size_t char_len = nex_rt_utf8_codepoint_len(value, byte_idx);
    
    char *result = (char *)malloc(char_len + 1);
    if (result == NULL) return NULL;
    memcpy(result, value + byte_idx, char_len);
    result[char_len] = '\0';
    return result;
}

/* Unicode-aware substring by codepoint indices */
char *nex_rt_utf8_substr(const char *value, long long start, long long end) {
    if (value == NULL) return NULL;
    size_t char_count = nex_rt_utf8_count(value);
    if (start < 0) start = 0;
    if (end > (long long)char_count) end = (long long)char_count;
    if (end < start) { end = start; }
    
    size_t start_byte = nex_rt_utf8_byte_offset(value, (size_t)start);
    size_t end_byte = nex_rt_utf8_byte_offset(value, (size_t)end);
    size_t count = end_byte - start_byte;
    
    char *result = (char *)malloc(count + 1);
    if (result == NULL) return NULL;
    memcpy(result, value + start_byte, count);
    result[count] = '\0';
    return result;
}

/* Unicode-aware character code at codepoint index */
long long nex_rt_utf8_char_code_at(const char *value, long long index) {
    if (value == NULL || index < 0) return -1;
    size_t char_count = nex_rt_utf8_count(value);
    if ((size_t)index >= char_count) return -1;
    size_t byte_idx = nex_rt_utf8_byte_offset(value, (size_t)index);
    return nex_rt_utf8_codepoint_at(value, byte_idx);
}

long long nex_rt_char_code(const char *value) {
    if (value == NULL || value[0] == '\0') {
        return -1;
    }
    return nex_rt_utf8_codepoint_at(value, 0);
}

/* Unicode-aware string reverse: reverses code points, not bytes */
char *nex_rt_utf8_reverse(const char *value) {
    if (value == NULL) return NULL;
    size_t char_count = nex_rt_utf8_count(value);
    size_t byte_len = strlen(value);
    
    char *result = (char *)malloc(byte_len + 1);
    if (result == NULL) return NULL;
    
    size_t dst_byte = 0;
    for (size_t i = char_count; i > 0; i--) {
        size_t src_byte = nex_rt_utf8_byte_offset(value, i - 1);
        size_t char_len = nex_rt_utf8_codepoint_len(value, src_byte);
        memcpy(result + dst_byte, value + src_byte, char_len);
        dst_byte += char_len;
    }
    result[byte_len] = '\0';
    return result;
}

/* Growable arrays: a three-word header { data, len, cap } allocated by the
   compiler-generated code. Helpers exist so the IR never needs inttoptr. */
void *nex_rt_array_alloc(long long size) {
    if (size <= 0) {
        size = 1;
    }
    return malloc((size_t)size);
}

void *nex_rt_array_new(void) {
    return calloc(3, sizeof(void *));
}

void *nex_rt_array_grow(void *data, long long size) {
    if (size <= 0) {
        size = 1;
    }
    return realloc(data, (size_t)size);
}

/* Ensures capacity for `needed_len` elements of `elem_size` bytes, growing by
   doubling. The header is never reallocated, so the pointer stays valid. */
void *nex_rt_array_ensure(void *header, long long needed_len, long long elem_size) {
    if (header == NULL) {
        return NULL;
    }
    long long *words = (long long *)header;
    void **data = (void **)header;
    long long len = words[1];
    long long cap = words[2];
    (void)len;
    if (needed_len <= cap) {
        return header;
    }
    if (needed_len < 0 || elem_size < 0 || cap < 0) {
        return NULL;
    }
    long long new_cap = cap > 0 ? cap : 4;
    if (cap > 0) {
        /* Doubling an already-large capacity must not wrap to a negative
           value: that used to make the loop below spin forever or produce a
           tiny `realloc` that later writes ran past. */
        if ((size_t)cap > SIZE_MAX / 2) {
            return NULL;
        }
        new_cap = cap * 2;
    }
    while (new_cap < needed_len) {
        if ((size_t)new_cap > SIZE_MAX / 2) {
            new_cap = needed_len;
            break;
        }
        new_cap *= 2;
    }
    size_t element = (size_t)(elem_size > 0 ? elem_size : 1);
    if ((size_t)new_cap > SIZE_MAX / element) {
        return NULL;
    }
    size_t bytes = (size_t)new_cap * element;
    void *grown = realloc(data[0], bytes);
    if (grown == NULL) {
        return NULL;
    }
    data[0] = grown;
    words[2] = new_cap;
    return header;
}

/* ------------------------------------------------------------------------- */
/* Minimal JSON DOM. Nodes are heap-allocated and addressed by handle (i64).  */
/* Handles are never freed in this milestone: NEX has no ownership model yet.  */
/* ------------------------------------------------------------------------- */

typedef enum {
    NEX_JSON_NULL = 0,
    NEX_JSON_BOOL,
    NEX_JSON_NUMBER,
    NEX_JSON_STRING,
    NEX_JSON_ARRAY,
    NEX_JSON_OBJECT
} nex_json_kind;

typedef struct nex_json_node {
    nex_json_kind kind;
    double number;
    int boolean;
    char *string;
    struct nex_json_node **items;
    char **keys;
    size_t length;
} nex_json_node;

typedef struct {
    const char *text;
    size_t position;
    int failed;
} nex_json_parser;

static void nex_json_skip(nex_json_parser *parser) {
    while (parser->text[parser->position] != '\0') {
        char c = parser->text[parser->position];
        if (c == ' ' || c == '\t' || c == '\r' || c == '\n') {
            parser->position++;
        } else {
            break;
        }
    }
}

static nex_json_node *nex_json_new(nex_json_kind kind) {
    nex_json_node *node = (nex_json_node *)calloc(1, sizeof(nex_json_node));
    if (node != NULL) {
        node->kind = kind;
    }
    return node;
}

static char *nex_json_parse_string_raw(nex_json_parser *parser) {
    if (parser->text[parser->position] != '"') {
        parser->failed = 1;
        return NULL;
    }
    parser->position++;
    size_t capacity = 16;
    size_t length = 0;
    char *buffer = (char *)malloc(capacity);
    if (buffer == NULL) {
        parser->failed = 1;
        return NULL;
    }
    while (parser->text[parser->position] != '\0') {
        char c = parser->text[parser->position];
        if (c == '"') {
            parser->position++;
            buffer[length] = '\0';
            return buffer;
        }
        if (c == '\\') {
            parser->position++;
            char escaped = parser->text[parser->position];
            char decoded;
            switch (escaped) {
                case '"': decoded = '"'; break;
                case '\\': decoded = '\\'; break;
                case '/': decoded = '/'; break;
                case 'b': decoded = '\b'; break;
                case 'f': decoded = '\f'; break;
                case 'n': decoded = '\n'; break;
                case 'r': decoded = '\r'; break;
                case 't': decoded = '\t'; break;
                case 'u': {
                    /* Decode a \uXXXX escape into UTF-8. */
                    if (parser->text[parser->position + 1] == '\0' ||
                        parser->text[parser->position + 4] == '\0') {
                        free(buffer);
                        parser->failed = 1;
                        return NULL;
                    }
                    char hex[5];
                    for (int i = 0; i < 4; i++) {
                        hex[i] = parser->text[parser->position + 1 + i];
                    }
                    hex[4] = '\0';
                    unsigned int codepoint = (unsigned int)strtoul(hex, NULL, 16);
                    parser->position += 4;
                    if (length + 4 >= capacity) {
                        capacity *= 2;
                        char *grown = (char *)realloc(buffer, capacity);
                        if (grown == NULL) {
                            free(buffer);
                            parser->failed = 1;
                            return NULL;
                        }
                        buffer = grown;
                    }
                    if (codepoint < 0x80) {
                        buffer[length++] = (char)codepoint;
                    } else if (codepoint < 0x800) {
                        buffer[length++] = (char)(0xC0 | (codepoint >> 6));
                        buffer[length++] = (char)(0x80 | (codepoint & 0x3F));
                    } else {
                        buffer[length++] = (char)(0xE0 | (codepoint >> 12));
                        buffer[length++] = (char)(0x80 | ((codepoint >> 6) & 0x3F));
                        buffer[length++] = (char)(0x80 | (codepoint & 0x3F));
                    }
                    parser->position++;
                    continue;
                }
                default:
                    free(buffer);
                    parser->failed = 1;
                    return NULL;
            }
            parser->position++;
            if (length + 1 >= capacity) {
                capacity *= 2;
                char *grown = (char *)realloc(buffer, capacity);
                if (grown == NULL) {
                    free(buffer);
                    parser->failed = 1;
                    return NULL;
                }
                buffer = grown;
            }
            buffer[length++] = decoded;
            continue;
        }
        if (length + 1 >= capacity) {
            capacity *= 2;
            char *grown = (char *)realloc(buffer, capacity);
            if (grown == NULL) {
                free(buffer);
                parser->failed = 1;
                return NULL;
            }
            buffer = grown;
        }
        buffer[length++] = c;
        parser->position++;
    }
    free(buffer);
    parser->failed = 1;
    return NULL;
}

static nex_json_node *nex_json_parse_value(nex_json_parser *parser);

static nex_json_node *nex_json_parse_array(nex_json_parser *parser) {
    parser->position++; /* [ */
    nex_json_node *node = nex_json_new(NEX_JSON_ARRAY);
    if (node == NULL) {
        parser->failed = 1;
        return NULL;
    }
    size_t capacity = 4;
    node->items = (nex_json_node **)malloc(capacity * sizeof(nex_json_node *));
    nex_json_skip(parser);
    if (parser->text[parser->position] == ']') {
        parser->position++;
        return node;
    }
    for (;;) {
        nex_json_node *item = nex_json_parse_value(parser);
        if (item == NULL || parser->failed) {
            parser->failed = 1;
            return NULL;
        }
        if (node->length == capacity) {
            capacity *= 2;
            nex_json_node **grown =
                (nex_json_node **)realloc(node->items, capacity * sizeof(nex_json_node *));
            if (grown == NULL) {
                parser->failed = 1;
                return NULL;
            }
            node->items = grown;
        }
        node->items[node->length++] = item;
        nex_json_skip(parser);
        char c = parser->text[parser->position];
        parser->position++;
        if (c == ']') {
            return node;
        }
        if (c != ',') {
            parser->failed = 1;
            return NULL;
        }
        nex_json_skip(parser);
    }
}

static nex_json_node *nex_json_parse_object(nex_json_parser *parser) {
    parser->position++; /* { */
    nex_json_node *node = nex_json_new(NEX_JSON_OBJECT);
    if (node == NULL) {
        parser->failed = 1;
        return NULL;
    }
    size_t capacity = 4;
    node->items = (nex_json_node **)malloc(capacity * sizeof(nex_json_node *));
    node->keys = (char **)malloc(capacity * sizeof(char *));
    nex_json_skip(parser);
    if (parser->text[parser->position] == '}') {
        parser->position++;
        return node;
    }
    for (;;) {
        char *key = nex_json_parse_string_raw(parser);
        if (key == NULL || parser->failed) {
            parser->failed = 1;
            return NULL;
        }
        nex_json_skip(parser);
        if (parser->text[parser->position] != ':') {
            parser->failed = 1;
            return NULL;
        }
        parser->position++;
        nex_json_node *value = nex_json_parse_value(parser);
        if (value == NULL || parser->failed) {
            parser->failed = 1;
            return NULL;
        }
        if (node->length == capacity) {
            capacity *= 2;
            nex_json_node **grown_items =
                (nex_json_node **)realloc(node->items, capacity * sizeof(nex_json_node *));
            char **grown_keys = (char **)realloc(node->keys, capacity * sizeof(char *));
            if (grown_items == NULL || grown_keys == NULL) {
                parser->failed = 1;
                return NULL;
            }
            node->items = grown_items;
            node->keys = grown_keys;
        }
        node->keys[node->length] = key;
        node->items[node->length] = value;
        node->length++;
        nex_json_skip(parser);
        char c = parser->text[parser->position];
        parser->position++;
        if (c == '}') {
            return node;
        }
        if (c != ',') {
            parser->failed = 1;
            return NULL;
        }
        nex_json_skip(parser);
    }
}

static nex_json_node *nex_json_parse_value(nex_json_parser *parser) {
    nex_json_skip(parser);
    char c = parser->text[parser->position];
    if (c == '{') {
        return nex_json_parse_object(parser);
    }
    if (c == '[') {
        return nex_json_parse_array(parser);
    }
    if (c == '"') {
        char *text = nex_json_parse_string_raw(parser);
        if (text == NULL) {
            parser->failed = 1;
            return NULL;
        }
        nex_json_node *node = nex_json_new(NEX_JSON_STRING);
        if (node == NULL) {
            free(text);
            parser->failed = 1;
            return NULL;
        }
        node->string = text;
        return node;
    }
    if (c == 't' && strncmp(parser->text + parser->position, "true", 4) == 0) {
        parser->position += 4;
        nex_json_node *node = nex_json_new(NEX_JSON_BOOL);
        if (node != NULL) {
            node->boolean = 1;
        }
        return node;
    }
    if (c == 'f' && strncmp(parser->text + parser->position, "false", 5) == 0) {
        parser->position += 5;
        return nex_json_new(NEX_JSON_BOOL);
    }
    if (c == 'n' && strncmp(parser->text + parser->position, "null", 4) == 0) {
        parser->position += 4;
        return nex_json_new(NEX_JSON_NULL);
    }
    /* number */
    char *end = NULL;
    double value = strtod(parser->text + parser->position, &end);
    if (end == parser->text + parser->position) {
        parser->failed = 1;
        return NULL;
    }
    parser->position = (size_t)(end - parser->text);
    nex_json_node *node = nex_json_new(NEX_JSON_NUMBER);
    if (node != NULL) {
        node->number = value;
    }
    return node;
}

long long nex_rt_json_parse(const char *text) {
    if (text == NULL) {
        return 0;
    }
    nex_json_parser parser;
    parser.text = text;
    parser.position = 0;
    parser.failed = 0;
    nex_json_node *root = nex_json_parse_value(&parser);
    if (root == NULL || parser.failed) {
        return 0;
    }
    nex_json_skip(&parser);
    if (parser.text[parser.position] != '\0') {
        return 0;
    }
    return (long long)root;
}

static nex_json_node *nex_json_handle(long long handle) {
    if (handle <= 0) {
        return NULL;
    }
    return (nex_json_node *)handle;
}

char *nex_rt_json_empty_string(void) {
    char *empty = (char *)malloc(1);
    if (empty != NULL) {
        empty[0] = '\0';
    }
    return empty;
}

char *nex_rt_json_type(long long handle) {
    static const char *names[] = {"null", "bool", "number", "string", "array", "object"};
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL || node->kind < 0 || node->kind > NEX_JSON_OBJECT) {
        return nex_rt_json_empty_string();
    }
    size_t length = strlen(names[node->kind]);
    char *copy = (char *)malloc(length + 1);
    if (copy != NULL) {
        memcpy(copy, names[node->kind], length + 1);
    }
    return copy;
}

long long nex_rt_json_len(long long handle) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL) {
        return 0;
    }
    return (long long)node->length;
}

long long nex_rt_json_at(long long handle, long long index) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL || node->kind != NEX_JSON_ARRAY) {
        return 0;
    }
    if (index < 0 || (size_t)index >= node->length) {
        return 0;
    }
    return (long long)node->items[index];
}

long long nex_rt_json_get(long long handle, const char *key) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL || node->kind != NEX_JSON_OBJECT || key == NULL) {
        return 0;
    }
    for (size_t i = 0; i < node->length; i++) {
        if (strcmp(node->keys[i], key) == 0) {
            return (long long)node->items[i];
        }
    }
    return 0;
}

char *nex_rt_json_key(long long handle) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL) {
        return NULL;
    }
    return node->string;
}

char *nex_rt_json_str(long long handle) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL) {
        return NULL;
    }
    if (node->kind == NEX_JSON_STRING) {
        size_t length = strlen(node->string);
        char *copy = (char *)malloc(length + 1);
        if (copy != NULL) {
            memcpy(copy, node->string, length + 1);
        }
        return copy;
    }
    return NULL;
}

double nex_rt_json_num(long long handle) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL) {
        return 0;
    }
    return node->number;
}

long long nex_rt_json_bool(long long handle) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL) {
        return 0;
    }
    return node->boolean;
}

long long nex_rt_json_is_null(long long handle) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL) {
        return 1;
    }
    return node->kind == NEX_JSON_NULL;
}

/* ------------------------------------------------------------------------- */
/* Splitting helpers: the collection itself is built in NEX with growable      */
/* arrays, so these only locate the nth field.                                 */
/* ------------------------------------------------------------------------- */

long long nex_rt_str_split_count(const char *value, const char *separator) {
    if (value == NULL || separator == NULL) {
        return 0;
    }
    size_t step = strlen(separator);
    if (step == 0) {
        return 0;
    }
    long long count = 1;
    const char *cursor = value;
    const char *found;
    while ((found = strstr(cursor, separator)) != NULL) {
        count++;
        cursor = found + step;
    }
    return count;
}

char *nex_rt_str_split_at(const char *value, const char *separator, long long index) {
    if (value == NULL || separator == NULL || index < 0) {
        return NULL;
    }
    size_t step = strlen(separator);
    if (step == 0) {
        return NULL;
    }
    long long current = 0;
    const char *start = value;
    const char *cursor = value;
    const char *found;
    while ((found = strstr(cursor, separator)) != NULL) {
        if (current == index) {
            size_t length = (size_t)(found - start);
            char *token = (char *)malloc(length + 1);
            if (token == NULL) {
                return NULL;
            }
            memcpy(token, start, length);
            token[length] = '\0';
            return token;
        }
        current++;
        cursor = found + step;
        start = cursor;
    }
    if (current == index) {
        size_t length = strlen(start);
        char *token = (char *)malloc(length + 1);
        if (token == NULL) {
            return NULL;
        }
        memcpy(token, start, length);
        token[length] = '\0';
        return token;
    }
    return NULL;
}

/* Serialises a JSON node back to compact JSON text. */
static void nex_json_write_string(const char *text, char **buffer, size_t *length, size_t *capacity) {
    char escaped[8];
    for (const unsigned char *cursor = (const unsigned char *)text; *cursor != '\0'; cursor++) {
        const char *replacement = NULL;
        switch (*cursor) {
            case '"': replacement = "\\\""; break;
            case '\\': replacement = "\\\\"; break;
            case '\n': replacement = "\\n"; break;
            case '\r': replacement = "\\r"; break;
    case '\t': replacement = "\\t"; break;
            default: break;
        }
        if (replacement != NULL) {
            size_t extra = strlen(replacement);
            if (*length + extra + 1 >= *capacity) {
                *capacity = (*capacity + extra + 1) * 2;
                char *grown = (char *)realloc(*buffer, *capacity);
                if (grown == NULL) {
                    return;
                }
                *buffer = grown;
            }
            memcpy(*buffer + *length, replacement, extra);
            *length += extra;
            continue;
        }
        (void)escaped;
        if (*length + 2 >= *capacity) {
            *capacity *= 2;
            char *grown = (char *)realloc(*buffer, *capacity);
            if (grown == NULL) {
                return;
            }
            *buffer = grown;
        }
        (*buffer)[(*length)++] = (char)*cursor;
    }
}

static void nex_json_write_escaped(const char *text, char **buffer, size_t *length, size_t *capacity) {
    nex_json_write_string(text, buffer, length, capacity);
}

static void nex_json_append(char **buffer, size_t *length, size_t *capacity, const char *text) {
    size_t extra = strlen(text);
    if (*length + extra + 1 >= *capacity) {
        while (*capacity < *length + extra + 1) {
            *capacity *= 2;
        }
        char *grown = (char *)realloc(*buffer, *capacity);
        if (grown == NULL) {
            return;
        }
        *buffer = grown;
    }
    memcpy(*buffer + *length, text, extra);
    *length += extra;
}

static void nex_json_write_value(nex_json_node *node, char **buffer, size_t *length, size_t *capacity);

static void nex_json_write_number(double value, char **buffer, size_t *length, size_t *capacity) {
    char text[64];
    if (value == (double)(long long)value) {
        snprintf(text, sizeof(text), "%lld", (long long)value);
    } else {
        snprintf(text, sizeof(text), "%g", value);
    }
    nex_json_append(buffer, length, capacity, text);
}

static void nex_json_write_value(nex_json_node *node, char **buffer, size_t *length, size_t *capacity) {
    if (node == NULL) {
        nex_json_append(buffer, length, capacity, "null");
        return;
    }
    switch (node->kind) {
        case NEX_JSON_NULL:
            nex_json_append(buffer, length, capacity, "null");
            break;
        case NEX_JSON_BOOL:
            nex_json_append(buffer, length, capacity, node->boolean ? "true" : "false");
            break;
        case NEX_JSON_NUMBER:
            nex_json_write_number(node->number, buffer, length, capacity);
            break;
        case NEX_JSON_STRING:
            nex_json_append(buffer, length, capacity, "\"");
            nex_json_write_escaped(node->string, buffer, length, capacity);
            nex_json_append(buffer, length, capacity, "\"");
            break;
        case NEX_JSON_ARRAY:
            nex_json_append(buffer, length, capacity, "[");
            for (size_t i = 0; i < node->length; i++) {
                if (i > 0) {
                    nex_json_append(buffer, length, capacity, ",");
                }
                nex_json_write_value(node->items[i], buffer, length, capacity);
            }
            nex_json_append(buffer, length, capacity, "]");
            break;
        case NEX_JSON_OBJECT:
            nex_json_append(buffer, length, capacity, "{");
            for (size_t i = 0; i < node->length; i++) {
                if (i > 0) {
                    nex_json_append(buffer, length, capacity, ",");
                }
                nex_json_append(buffer, length, capacity, "\"");
                nex_json_write_escaped(node->keys[i], buffer, length, capacity);
                nex_json_append(buffer, length, capacity, "\":");
                nex_json_write_value(node->items[i], buffer, length, capacity);
            }
            nex_json_append(buffer, length, capacity, "}");
            break;
    }
}

char *nex_rt_json_stringify(long long handle) {
    nex_json_node *node = nex_json_handle(handle);
    if (node == NULL) {
        return nex_rt_json_empty_string();
    }
    size_t capacity = 64;
    size_t length = 0;
    char *buffer = (char *)malloc(capacity);
    if (buffer == NULL) {
        return NULL;
    }
    nex_json_write_value(node, &buffer, &length, &capacity);
    if (buffer == NULL) {
        return NULL;
    }
    if (length + 1 >= capacity) {
        char *grown = (char *)realloc(buffer, length + 1);
        if (grown != NULL) {
            buffer = grown;
        }
    }
    if (buffer != NULL) {
        buffer[length] = '\0';
    }
    return buffer;
}

/* ------------------------------------------------------------------------- */
/* String-keyed string-valued hash map.                                       */
/* Handles are pointers; entries are never freed (no ownership model yet).      */
/* ------------------------------------------------------------------------- */

typedef struct nex_map_entry {
    char *key;
    char *value;
    int occupied;
} nex_map_entry;

typedef struct nex_map {
    nex_map_entry *entries;
    size_t capacity;
    size_t length;
} nex_map;

static unsigned long long nex_map_hash(const char *key) {
    unsigned long long hash = 1469598103934665603ULL;
    for (const unsigned char *cursor = (const unsigned char *)key; *cursor != '\0'; cursor++) {
        hash ^= *cursor;
        hash *= 1099511628211ULL;
    }
    return hash;
}

long long nex_rt_map_new(long long capacity) {
    if (capacity < 8) {
        capacity = 8;
    }
    nex_map *map = (nex_map *)calloc(1, sizeof(nex_map));
    if (map == NULL) {
        return 0;
    }
    map->capacity = (size_t)capacity;
    map->entries = (nex_map_entry *)calloc(map->capacity, sizeof(nex_map_entry));
    if (map->entries == NULL) {
        free(map);
        return 0;
    }
    return (long long)map;
}

static nex_map *nex_map_handle(long long handle) {
    if (handle <= 0) {
        return NULL;
    }
    return (nex_map *)handle;
}

static char *nex_map_copy(const char *text) {
    if (text == NULL) {
        text = "";
    }
    size_t length = strlen(text);
    char *copy = (char *)malloc(length + 1);
    if (copy != NULL) {
        memcpy(copy, text, length + 1);
    }
    return copy;
}

static void nex_map_grow(nex_map *map);

long long nex_rt_map_set(long long handle, const char *key, const char *value) {
    nex_map *map = nex_map_handle(handle);
    if (map == NULL || key == NULL) {
        return 0;
    }
    if (map->length * 2 >= map->capacity) {
        nex_map_grow(map);
    }
    unsigned long long hash = nex_map_hash(key);
    for (;;) {
        size_t index = (size_t)(hash & (map->capacity - 1));
        nex_map_entry *entry = &map->entries[index];
        if (!entry->occupied) {
            entry->occupied = 1;
            entry->key = nex_map_copy(key);
            entry->value = nex_map_copy(value);
            map->length++;
            return 1;
        }
        if (strcmp(entry->key, key) == 0) {
            char *copy = nex_map_copy(value);
            if (copy == NULL) {
                return 0;
            }
            free(entry->value);
            entry->value = copy;
            return 1;
        }
        hash++;
    }
}

char *nex_rt_map_get(long long handle, const char *key) {
    nex_map *map = nex_map_handle(handle);
    if (map == NULL || key == NULL) {
        return nex_rt_json_empty_string();
    }
    unsigned long long hash = nex_map_hash(key);
    for (;;) {
        size_t index = (size_t)(hash & (map->capacity - 1));
        nex_map_entry *entry = &map->entries[index];
        if (!entry->occupied) {
            return nex_rt_json_empty_string();
        }
        if (strcmp(entry->key, key) == 0) {
            return nex_map_copy(entry->value);
        }
        hash++;
    }
}

long long nex_rt_map_has(long long handle, const char *key) {
    nex_map *map = nex_map_handle(handle);
    if (map == NULL || key == NULL) {
        return 0;
    }
    unsigned long long hash = nex_map_hash(key);
    for (;;) {
        size_t index = (size_t)(hash & (map->capacity - 1));
        nex_map_entry *entry = &map->entries[index];
        if (!entry->occupied) {
            return 0;
        }
        if (strcmp(entry->key, key) == 0) {
            return 1;
        }
        hash++;
    }
}

long long nex_rt_map_remove(long long handle, const char *key) {
    nex_map *map = nex_map_handle(handle);
    if (map == NULL || key == NULL) {
        return 0;
    }
    unsigned long long hash = nex_map_hash(key);
    for (;;) {
        size_t index = (size_t)(hash & (map->capacity - 1));
        nex_map_entry *entry = &map->entries[index];
        if (!entry->occupied) {
            return 0;
        }
        if (strcmp(entry->key, key) == 0) {
            free(entry->key);
            free(entry->value);
            entry->key = NULL;
            entry->value = NULL;
            entry->occupied = 0;
            map->length--;
            return 1;
        }
        hash++;
    }
}

long long nex_rt_map_len(long long handle) {
    nex_map *map = nex_map_handle(handle);
    if (map == NULL) {
        return 0;
    }
    return (long long)map->length;
}

char *nex_rt_map_key_at(long long handle, long long index) {
    nex_map *map = nex_map_handle(handle);
    if (map == NULL || index < 0) {
        return nex_rt_json_empty_string();
    }
    long long current = 0;
    for (size_t i = 0; i < map->capacity; i++) {
        if (!map->entries[i].occupied) {
            continue;
        }
        if (current == index) {
            return nex_map_copy(map->entries[i].key);
        }
        current++;
    }
    return nex_rt_json_empty_string();
}

static void nex_map_grow(nex_map *map) {
    size_t old_capacity = map->capacity;
    nex_map_entry *old_entries = map->entries;
    map->capacity *= 2;
    map->entries = (nex_map_entry *)calloc(map->capacity, sizeof(nex_map_entry));
    if (map->entries == NULL) {
        map->entries = old_entries;
        map->capacity = old_capacity;
        return;
    }
    for (size_t i = 0; i < old_capacity; i++) {
        if (!old_entries[i].occupied) {
            continue;
        }
        unsigned long long hash = nex_map_hash(old_entries[i].key);
        for (;;) {
            size_t index = (size_t)(hash & (map->capacity - 1));
            if (!map->entries[index].occupied) {
                map->entries[index] = old_entries[i];
                break;
            }
            hash++;
        }
    }
    free(old_entries);
}

/* ------------------------------------------------------------------------- */
/* Threads. NEX passes a function symbol as an integer handle; the runtime     */
/* trampoline calls it. One optional i64 argument is supported.                */
/* ------------------------------------------------------------------------- */

long long nex_rt_thread_spawn_arg(long long entry, long long argument);

typedef struct nex_thread_state {
#ifdef _WIN32
    HANDLE handle;
#else
    pthread_t thread;
#endif
    void (*entry)(void);
    void (*entry_arg)(long long);
    long long argument;
    int done;
} nex_thread_state;

#ifdef _WIN32
static DWORD WINAPI nex_thread_trampoline(LPVOID parameter) {
    nex_thread_state *state = (nex_thread_state *)parameter;
    if (state->entry != NULL) {
        state->entry();
    } else if (state->entry_arg != NULL) {
        state->entry_arg(state->argument);
    }
    state->done = 1;
    return 0;
}

static long long nex_thread_start(nex_thread_state *state) {
    state->handle = CreateThread(NULL, 0, nex_thread_trampoline, state, 0, NULL);
    if (state->handle == NULL) {
        free(state);
        return 0;
    }
    return (long long)state;
}

long long nex_rt_thread_spawn(long long entry) {
    return nex_rt_thread_spawn_arg(entry, 0);
}

long long nex_rt_thread_spawn_arg(long long entry, long long argument) {
    if (entry == 0) {
        return 0;
    }
    nex_thread_state *state = (nex_thread_state *)calloc(1, sizeof(nex_thread_state));
    if (state == NULL) {
        return 0;
    }
    state->entry_arg = (void (*)(long long))entry;
    state->argument = argument;
    return nex_thread_start(state);
}

long long nex_rt_thread_spawn_with_arg(long long entry, long long argument) {
    if (entry == 0) {
        return 0;
    }
    nex_thread_state *state = (nex_thread_state *)calloc(1, sizeof(nex_thread_state));
    if (state == NULL) {
        return 0;
    }
    state->entry_arg = (void (*)(long long))entry;
    state->argument = argument;
    return nex_thread_start(state);
}

long long nex_rt_thread_join(long long handle) {
    if (handle == 0) {
        return 0;
    }
    nex_thread_state *state = (nex_thread_state *)handle;
    WaitForSingleObject(state->handle, INFINITE);
    CloseHandle(state->handle);
    int done = state->done;
    free(state);
    return done;
}

long long nex_rt_thread_running(long long handle) {
    if (handle == 0) {
        return 0;
    }
    nex_thread_state *state = (nex_thread_state *)handle;
    return WaitForSingleObject(state->handle, 0) == WAIT_TIMEOUT;
}
#else
#include <pthread.h>

static void *nex_thread_trampoline(void *parameter) {
    nex_thread_state *state = (nex_thread_state *)parameter;
    if (state->entry != NULL) {
        state->entry();
    } else if (state->entry_arg != NULL) {
        state->entry_arg(state->argument);
    }
    state->done = 1;
    return NULL;
}

static long long nex_thread_start(nex_thread_state *state) {
    if (pthread_create(&state->thread, NULL, nex_thread_trampoline, state) != 0) {
        free(state);
        return 0;
    }
    return (long long)state;
}

long long nex_rt_thread_spawn(long long entry) {
    if (entry == 0) {
        return 0;
    }
    nex_thread_state *state = (nex_thread_state *)calloc(1, sizeof(nex_thread_state));
    if (state == NULL) {
        return 0;
    }
    state->entry = (void (*)(void))entry;
    return nex_thread_start(state);
}

long long nex_rt_thread_spawn_arg(long long entry, long long argument) {
    if (entry == 0) {
        return 0;
    }
    nex_thread_state *state = (nex_thread_state *)calloc(1, sizeof(nex_thread_state));
    if (state == NULL) {
        return 0;
    }
    state->entry_arg = (void (*)(long long))entry;
    state->argument = argument;
    return nex_thread_start(state);
}

long long nex_rt_thread_join(long long handle) {
    if (handle == 0) {
        return 0;
    }
    nex_thread_state *state = (nex_thread_state *)handle;
    int result = pthread_join(state->thread, NULL);
    int done = state->done;
    free(state);
    return result == 0 && done;
}

long long nex_rt_thread_running(long long handle) {
    if (handle == 0) {
        return 0;
    }
    nex_thread_state *state = (nex_thread_state *)handle;
    return state->done ? 0 : 1;
}
#endif

/* ------------------------------------------------------------------------- */
/* Mutexes. Handles are heap-allocated; no ownership model yet.                */
/* ------------------------------------------------------------------------- */

#ifdef _WIN32
long long nex_rt_mutex_new(void) {
    CRITICAL_SECTION *mutex = (CRITICAL_SECTION *)malloc(sizeof(CRITICAL_SECTION));
    if (mutex == NULL) {
        return 0;
    }
    InitializeCriticalSection(mutex);
    return (long long)mutex;
}

void nex_rt_mutex_lock(long long handle) {
    if (handle != 0) {
        EnterCriticalSection((CRITICAL_SECTION *)handle);
    }
}

void nex_rt_mutex_unlock(long long handle) {
    if (handle != 0) {
        LeaveCriticalSection((CRITICAL_SECTION *)handle);
    }
}
#else
long long nex_rt_mutex_new(void) {
    pthread_mutex_t *mutex = (pthread_mutex_t *)malloc(sizeof(pthread_mutex_t));
    if (mutex == NULL) {
        return 0;
    }
    if (pthread_mutex_init(mutex, NULL) != 0) {
        free(mutex);
        return 0;
    }
    return (long long)mutex;
}

void nex_rt_mutex_lock(long long handle) {
    if (handle != 0) {
        pthread_mutex_lock((pthread_mutex_t *)handle);
    }
}

void nex_rt_mutex_unlock(long long handle) {
    if (handle != 0) {
        pthread_mutex_unlock((pthread_mutex_t *)handle);
    }
}
#endif

/* ------------------------------------------------------------------------- */
/* Global slots. NEX has module-level `var` bindings; the compiler assigns    */
/* each one a slot index and accesses it through these typed accessors.        */
/* ------------------------------------------------------------------------- */

#define NEX_MAX_GLOBALS 1024

static long long nex_globals_i64[NEX_MAX_GLOBALS];
static double nex_globals_f64[NEX_MAX_GLOBALS];
static char *nex_globals_str[NEX_MAX_GLOBALS];

void nex_rt_global_set_i64(long long slot, long long value) {
    if (slot >= 0 && slot < NEX_MAX_GLOBALS) {
        nex_globals_i64[slot] = value;
    }
}

long long nex_rt_global_get_i64(long long slot) {
    if (slot >= 0 && slot < NEX_MAX_GLOBALS) {
        return nex_globals_i64[slot];
    }
    return 0;
}

void nex_rt_global_set_f64(long long slot, double value) {
    if (slot >= 0 && slot < NEX_MAX_GLOBALS) {
        nex_globals_f64[slot] = value;
    }
}

double nex_rt_global_get_f64(long long slot) {
    if (slot >= 0 && slot < NEX_MAX_GLOBALS) {
        return nex_globals_f64[slot];
    }
    return 0;
}

void nex_rt_global_set_str(long long slot, const char *value) {
    if (slot < 0 || slot >= NEX_MAX_GLOBALS) {
        return;
    }
    free(nex_globals_str[slot]);
    size_t length = strlen(value);
    char *copy = (char *)malloc(length + 1);
    if (copy != NULL) {
        memcpy(copy, value, length + 1);
        nex_globals_str[slot] = copy;
    }
}

char *nex_rt_global_get_str(long long slot) {
    static char empty[1] = {0};
    if (slot >= 0 && slot < NEX_MAX_GLOBALS && nex_globals_str[slot] != NULL) {
        size_t length = strlen(nex_globals_str[slot]);
        char *copy = (char *)malloc(length + 1);
        if (copy != NULL) {
            memcpy(copy, nex_globals_str[slot], length + 1);
            return copy;
        }
    }
    return empty;
}

/* ------------------------------------------------------------------------- */
/* Router: method + pattern matching with `:param` capture.                    */
/* nex_rt_router_match returns 0 for no match, -1 for a path that exists under */
/* a different method (405), and a 1-based route id otherwise.                 */
/* ------------------------------------------------------------------------- */

#define NEX_ROUTER_MAX_PARAMS 16

typedef struct {
    char *method;
    char *pattern;
} nex_route;

typedef struct {
    nex_route *routes;
    size_t length;
    size_t capacity;
} nex_router;

/* Captured params are per thread: two connections matched at the same time must
   not overwrite each other's values. */
static _Thread_local char *nex_tls_params[NEX_ROUTER_MAX_PARAMS];
static _Thread_local char *nex_tls_values[NEX_ROUTER_MAX_PARAMS];
static _Thread_local int nex_tls_param_count;

/* Frees the captures owned by the current thread and resets the count. The
   strings were allocated by `nex_router_split`; ownership for matched params
   moved to the TLS slots in `nex_rt_router_match`. */
static void nex_router_clear_params(void) {
    for (int i = 0; i < nex_tls_param_count && i < NEX_ROUTER_MAX_PARAMS; i++) {
        free(nex_tls_params[i]);
        free(nex_tls_values[i]);
        nex_tls_params[i] = NULL;
        nex_tls_values[i] = NULL;
    }
    nex_tls_param_count = 0;
}

static char **nex_router_split(const char *text, size_t *count) {
    size_t capacity = 8;
    char **parts = (char **)malloc(capacity * sizeof(char *));
    size_t length = 0;
    const char *cursor = text;
    while (*cursor != '\0') {
        const char *end = cursor;
        while (*end != '\0' && *end != '/') {
            end++;
        }
        if (length == capacity) {
            capacity *= 2;
            char **grown = (char **)realloc(parts, capacity * sizeof(char *));
            if (grown == NULL) {
                break;
            }
            parts = grown;
        }
        size_t size = (size_t)(end - cursor);
        char *part = (char *)malloc(size + 1);
        if (part != NULL) {
            memcpy(part, cursor, size);
            part[size] = '\0';
        }
        parts[length++] = part;
        if (*end == '\0') {
            break;
        }
        cursor = end + 1;
    }
    *count = length;
    return parts;
}

static void nex_router_free_parts(char **parts, size_t count) {
    if (parts == NULL) {
        return;
    }
    for (size_t i = 0; i < count; i++) {
        free(parts[i]);
    }
    free(parts);
}

long long nex_rt_router_new(void) {
    nex_router *router = (nex_router *)calloc(1, sizeof(nex_router));
    return (long long)router;
}

static nex_router *nex_router_handle(long long handle) {
    if (handle <= 0) {
        return NULL;
    }
    return (nex_router *)handle;
}

long long nex_rt_router_add(long long handle, const char *method, const char *pattern) {
    nex_router *router = nex_router_handle(handle);
    if (router == NULL || method == NULL || pattern == NULL) {
        return 0;
    }
    if (router->length == router->capacity) {
        size_t capacity = router->capacity == 0 ? 8 : router->capacity * 2;
        nex_route *grown = (nex_route *)realloc(router->routes, capacity * sizeof(nex_route));
        if (grown == NULL) {
            return 0;
        }
        router->routes = grown;
        router->capacity = capacity;
    }
    size_t length = strlen(pattern);
    char *copy = (char *)malloc(length + 1);
    if (copy == NULL) {
        return 0;
    }
    memcpy(copy, pattern, length + 1);
    router->routes[router->length].method = nex_map_copy(method);
    router->routes[router->length].pattern = copy;
    router->length++;
    return (long long)router->length;
}

long long nex_rt_router_match(long long handle, const char *method, const char *path) {
    nex_router *router = nex_router_handle(handle);
    if (router == NULL || method == NULL || path == NULL) {
        return 0;
    }
    /* Release the captures from the previous match on this thread. Without
       this, every matched request leaked its pattern and value strings for
       the lifetime of the process (a long-running server grew without bound). */
    nex_router_clear_params();
    size_t path_count = 0;
    char **path_parts = nex_router_split(path, &path_count);
    int path_matched = 0;

    for (size_t route_index = 0; route_index < router->length; route_index++) {
        nex_route *route = &router->routes[route_index];
        size_t pattern_count = 0;
        char **pattern_parts = nex_router_split(route->pattern, &pattern_count);
        if (pattern_count != path_count) {
            nex_router_free_parts(pattern_parts, pattern_count);
            continue;
        }
        char *params[NEX_ROUTER_MAX_PARAMS];
        char *values[NEX_ROUTER_MAX_PARAMS];
        int param_count = 0;
        int matched = 1;
        for (size_t i = 0; i < pattern_count; i++) {
            if (pattern_parts[i][0] == ':') {
                if (param_count >= NEX_ROUTER_MAX_PARAMS) {
                    /* More captures than the runtime can expose: treat the
                       route as non-matching instead of silently dropping
                       params (which produced wrong handler results). */
                    matched = 0;
                    break;
                }
                params[param_count] = pattern_parts[i];
                values[param_count] = path_parts[i];
                param_count++;
                continue;
            }
            if (strcmp(pattern_parts[i], path_parts[i]) != 0) {
                matched = 0;
                break;
            }
        }
        if (matched && strcmp(route->method, method) == 0) {
            nex_tls_param_count = 0;
            for (int i = 0; i < param_count; i++) {
                nex_tls_params[nex_tls_param_count] = params[i];
                nex_tls_values[nex_tls_param_count] = values[i];
                nex_tls_param_count++;
            }
            /* Ownership of the split parts moves to the router for params. */
            for (size_t i = 0; i < pattern_count; i++) {
                if (pattern_parts[i][0] != ':') {
                    free(pattern_parts[i]);
                }
            }
            free(pattern_parts);
            for (size_t i = 0; i < path_count; i++) {
                int is_value = 0;
                for (int p = 0; p < param_count; p++) {
                    if (values[p] == path_parts[i]) {
                        is_value = 1;
                        break;
                    }
                }
                if (!is_value) {
                    free(path_parts[i]);
                }
            }
            free(path_parts);
            return (long long)(route_index + 1);
        }
        nex_router_free_parts(pattern_parts, pattern_count);
        if (matched) {
            path_matched = 1;
        }
    }
    nex_router_free_parts(path_parts, path_count);
    if (path_matched) {
        return -1;
    }
    return 0;
}

char *nex_rt_router_param(long long handle, const char *name) {
    nex_router *router = nex_router_handle(handle);
    if (router == NULL || name == NULL) {
        return nex_rt_json_empty_string();
    }
    for (int i = 0; i < nex_tls_param_count; i++) {
        const char *key = nex_tls_params[i];
        if (key != NULL && key[0] == ':') {
            key++;
        }
        if (key != NULL && strcmp(key, name) == 0) {
            return nex_map_copy(nex_tls_values[i]);
        }
    }
    return nex_rt_json_empty_string();
}

/* ------------------------------------------------------------------------- */
/* Thread pool: fixed worker threads pulling {entry, argument} tasks.          */
/* Used instead of thread-per-connection so a server does not create a thread  */
/* for every socket.                                                          */
/* ------------------------------------------------------------------------- */

typedef struct nex_task {
    void (*entry)(long long);
    long long argument;
} nex_task;

typedef struct nex_pool {
    nex_task *tasks;
    size_t capacity;
    size_t head;
    size_t tail;
    size_t count;
    size_t workers;
    int shutdown;
#ifdef _WIN32
    CRITICAL_SECTION mutex;
    CONDITION_VARIABLE not_empty;
#else
    pthread_mutex_t mutex;
    pthread_cond_t not_empty;
#endif
} nex_pool;

#ifdef _WIN32

static DWORD WINAPI nex_pool_worker(LPVOID parameter) {
    nex_pool *pool = (nex_pool *)parameter;
    for (;;) {
        EnterCriticalSection(&pool->mutex);
        while (pool->count == 0 && !pool->shutdown) {
            SleepConditionVariableCS(&pool->not_empty, &pool->mutex, INFINITE);
        }
        if (pool->shutdown && pool->count == 0) {
            LeaveCriticalSection(&pool->mutex);
            return 0;
        }
        nex_task task = pool->tasks[pool->head];
        pool->head = (pool->head + 1) % pool->capacity;
        pool->count--;
        LeaveCriticalSection(&pool->mutex);
        task.entry(task.argument);
    }
}

long long nex_rt_pool_new(long long workers, long long capacity) {
    if (workers <= 0 || capacity <= 0) {
        return 0;
    }
    nex_pool *pool = (nex_pool *)calloc(1, sizeof(nex_pool));
    if (pool == NULL) {
        return 0;
    }
    pool->capacity = (size_t)capacity;
    pool->tasks = (nex_task *)calloc(pool->capacity, sizeof(nex_task));
    if (pool->tasks == NULL) {
        free(pool);
        return 0;
    }
    InitializeCriticalSection(&pool->mutex);
    InitializeConditionVariable(&pool->not_empty);
    for (long long i = 0; i < workers; i++) {
        HANDLE thread = CreateThread(NULL, 0, nex_pool_worker, pool, 0, NULL);
        if (thread == NULL) {
            break;
        }
        pool->workers++;
        CloseHandle(thread);
    }
    if (pool->workers == 0) {
        free(pool->tasks);
        free(pool);
        return 0;
    }
    return (long long)pool;
}

int nex_rt_pool_submit(long long handle, long long entry, long long argument) {
    if (handle == 0 || entry == 0) {
        return 0;
    }
    nex_pool *pool = (nex_pool *)handle;
    EnterCriticalSection(&pool->mutex);
    if (pool->count == pool->capacity) {
        LeaveCriticalSection(&pool->mutex);
        return 0;
    }
    pool->tasks[pool->tail].entry = (void (*)(long long))entry;
    pool->tasks[pool->tail].argument = argument;
    pool->tail = (pool->tail + 1) % pool->capacity;
    pool->count++;
    WakeConditionVariable(&pool->not_empty);
    LeaveCriticalSection(&pool->mutex);
    return 1;
}

#else

static void *nex_pool_worker(void *parameter) {
    nex_pool *pool = (nex_pool *)parameter;
    for (;;) {
        pthread_mutex_lock(&pool->mutex);
        while (pool->count == 0 && !pool->shutdown) {
            pthread_cond_wait(&pool->not_empty, &pool->mutex);
        }
        if (pool->shutdown && pool->count == 0) {
            pthread_mutex_unlock(&pool->mutex);
            return NULL;
        }
        nex_task task = pool->tasks[pool->head];
        pool->head = (pool->head + 1) % pool->capacity;
        pool->count--;
        pthread_mutex_unlock(&pool->mutex);
        task.entry(task.argument);
    }
}

long long nex_rt_pool_new(long long workers, long long capacity) {
    if (workers <= 0 || capacity <= 0) {
        return 0;
    }
    nex_pool *pool = (nex_pool *)calloc(1, sizeof(nex_pool));
    if (pool == NULL) {
        return 0;
    }
    pool->capacity = (size_t)capacity;
    pool->tasks = (nex_task *)calloc(pool->capacity, sizeof(nex_task));
    if (pool->tasks == NULL) {
        free(pool);
        return 0;
    }
    pthread_mutex_init(&pool->mutex, NULL);
    pthread_cond_init(&pool->not_empty, NULL);
    for (long long i = 0; i < workers; i++) {
        pthread_t thread;
        if (pthread_create(&thread, NULL, nex_pool_worker, pool) != 0) {
            break;
        }
        pthread_detach(thread);
        pool->workers++;
    }
    if (pool->workers == 0) {
        free(pool->tasks);
        free(pool);
        return 0;
    }
    return (long long)pool;
}

int nex_rt_pool_submit(long long handle, long long entry, long long argument) {
    if (handle == 0 || entry == 0) {
        return 0;
    }
    nex_pool *pool = (nex_pool *)handle;
    pthread_mutex_lock(&pool->mutex);
    if (pool->count == pool->capacity) {
        pthread_mutex_unlock(&pool->mutex);
        return 0;
    }
    pool->tasks[pool->tail].entry = (void (*)(long long))entry;
    pool->tasks[pool->tail].argument = argument;
    pool->tail = (pool->tail + 1) % pool->capacity;
    pool->count++;
    pthread_cond_signal(&pool->not_empty);
    pthread_mutex_unlock(&pool->mutex);
    return 1;
}

#endif

long long nex_rt_pool_workers(long long handle) {
    if (handle == 0) {
        return 0;
    }
    return (long long)((nex_pool *)handle)->workers;
}

/* Same as nex_rt_http_response but keeps the connection alive. */
char *nex_rt_http_response_keepalive(const char *status, const char *content_type, const char *body) {
    if (status == NULL) {
        status = "200 OK";
    }
    if (content_type == NULL || *content_type == '\0') {
        content_type = "text/plain; charset=utf-8";
    }
    if (body == NULL) {
        body = "";
    }
    size_t body_length = strlen(body);
    size_t head_length = strlen(status) + strlen(content_type) + 160;
    char *response = (char *)malloc(head_length + body_length + 1);
    if (response == NULL) {
        return NULL;
    }
    snprintf(
        response,
        head_length + body_length + 1,
        "HTTP/1.1 %s\r\nContent-Type: %s\r\nContent-Length: %zu\r\nConnection: close\r\n\r\n%s",
        status,
        content_type,
        body_length,
        body
    );
    return response;
}

/* Read timeout for a socket. Returns 1 on success, 0 on failure. */
long long nex_rt_tcp_set_timeout(long long socket, long long milliseconds) {
    if (socket < 0 || milliseconds < 0) {
        return 0;
    }

    /* Disable Nagle: without TCP_NODELAY each request/response pair pays the
       delayed-ACK round trip, which dominates every other cost on a small
       response. */
    int yes = 1;
#ifdef _WIN32
    setsockopt((SOCKET)socket, IPPROTO_TCP, TCP_NODELAY, (const char *)&yes, sizeof(yes));
#else
    setsockopt((int)socket, IPPROTO_TCP, TCP_NODELAY, &yes, sizeof(yes));
#endif

#ifdef _WIN32
    DWORD timeout = (DWORD)milliseconds;
    if (setsockopt((SOCKET)socket, SOL_SOCKET, SO_RCVTIMEO,
                   (const char *)&timeout, sizeof(timeout)) != 0) {
        return 0;
    }
    if (setsockopt((SOCKET)socket, SOL_SOCKET, SO_SNDTIMEO,
                   (const char *)&timeout, sizeof(timeout)) != 0) {
        return 0;
    }
    return 1;
#else
    struct timeval timeout;
    timeout.tv_sec = milliseconds / 1000;
    timeout.tv_usec = (milliseconds % 1000) * 1000;
    if (setsockopt((int)socket, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout)) != 0) {
        return 0;
    }
    if (setsockopt((int)socket, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout)) != 0) {
        return 0;
    }
    return 1;
#endif
}

/* ------------------------------------------------------------------------- */
/* Growable string builder: amortised O(1) append.                            */
/* `concat` is O(n) per call, so building a string in a loop with it copies    */
/* the whole prefix every iteration. A builder keeps one buffer and doubles.   */
/* ------------------------------------------------------------------------- */

typedef struct nex_builder {
    char *data;
    size_t length;
    size_t capacity;
} nex_builder;

long long nex_rt_builder_new(void) {
    nex_builder *builder = (nex_builder *)calloc(1, sizeof(nex_builder));
    if (builder == NULL) {
        return 0;
    }
    builder->capacity = 64;
    builder->data = (char *)malloc(builder->capacity);
    if (builder->data == NULL) {
        free(builder);
        return 0;
    }
    builder->data[0] = '\0';
    return (long long)builder;
}

static nex_builder *nex_builder_handle(long long handle) {
    if (handle <= 0) {
        return NULL;
    }
    return (nex_builder *)handle;
}

int nex_rt_builder_append(long long handle, const char *text) {
    nex_builder *builder = nex_builder_handle(handle);
    if (builder == NULL || text == NULL) {
        return 0;
    }
    size_t extra = strlen(text);
    if (builder->length + extra + 1 > builder->capacity) {
        size_t capacity = builder->capacity;
        while (capacity < builder->length + extra + 1) {
            capacity *= 2;
        }
        char *grown = (char *)realloc(builder->data, capacity);
        if (grown == NULL) {
            return 0;
        }
        builder->data = grown;
        builder->capacity = capacity;
    }
    memcpy(builder->data + builder->length, text, extra);
    builder->length += extra;
    builder->data[builder->length] = '\0';
    return 1;
}

long long nex_rt_builder_len(long long handle) {
    nex_builder *builder = nex_builder_handle(handle);
    if (builder == NULL) {
        return 0;
    }
    return (long long)builder->length;
}

char *nex_rt_builder_string(long long handle) {
    static char empty[1] = {0};
    nex_builder *builder = nex_builder_handle(handle);
    if (builder == NULL) {
        return empty;
    }
    char *copy = (char *)malloc(builder->length + 1);
    if (copy == NULL) {
        return empty;
    }
    memcpy(copy, builder->data, builder->length + 1);
    return copy;
}

void nex_rt_builder_clear(long long handle) {
    nex_builder *builder = nex_builder_handle(handle);
    if (builder != NULL) {
        builder->length = 0;
        builder->data[0] = '\0';
    }
}

/* ------------------------------------------------------------------------- */
/* JSON output helpers.                                                       */
/* `nex_rt_json_escape` escapes a string for embedding inside JSON quotes;     */
/* `nex_rt_json_quote` does both (quotes plus escaping).                       */
/* ------------------------------------------------------------------------- */

char *nex_rt_json_escape(const char *value) {
    if (value == NULL) {
        value = "";
    }
    size_t length = strlen(value);
    char *buffer = (char *)malloc(length * 2 + 1);
    if (buffer == NULL) {
        return NULL;
    }
    size_t written = 0;
    for (const unsigned char *cursor = (const unsigned char *)value; *cursor != '\0'; cursor++) {
        switch (*cursor) {
            case '"':
                buffer[written++] = '\\';
                buffer[written++] = '"';
                break;
            case '\\':
                buffer[written++] = '\\';
                buffer[written++] = '\\';
                break;
            case '\n':
                buffer[written++] = '\\';
                buffer[written++] = 'n';
                break;
            case '\r':
                buffer[written++] = '\\';
                buffer[written++] = 'r';
                break;
            case '\t':
                buffer[written++] = '\\';
                buffer[written++] = 't';
                break;
            default:
                if (*cursor < 0x20) {
                    /* Control characters become \u00XX. */
                    char digits[7];
                    snprintf(digits, sizeof(digits), "\\u%04x", (unsigned int)*cursor);
                    memcpy(buffer + written, digits, 6);
                    written += 6;
                } else {
                    buffer[written++] = (char)*cursor;
                }
                break;
        }
    }
    buffer[written] = '\0';
    return buffer;
}

char *nex_rt_json_quote(const char *value) {
    char *escaped = nex_rt_json_escape(value);
    if (escaped == NULL) {
        return NULL;
    }
    size_t length = strlen(escaped);
    char *quoted = (char *)malloc(length + 3);
    if (quoted == NULL) {
        free(escaped);
        return NULL;
    }
    quoted[0] = '"';
    memcpy(quoted + 1, escaped, length);
    quoted[length + 1] = '"';
    quoted[length + 2] = '\0';
    free(escaped);
    return quoted;
}

double nex_rt_floor_f64(double value) {
    return floor(value);
}

double nex_rt_ceil_f64(double value) {
    return ceil(value);
}

double nex_rt_round_f64(double value) {
    return round(value);
}

double nex_rt_trunc_f64(double value) {
    return trunc(value);
}

/* ------------------------------------------------------------------------- */
/* Process, environment, command-line arguments and logging.                 */
/* ------------------------------------------------------------------------- */

void nex_rt_init_args(int argc, char **argv) {
    nex_rt_argc = argc;
    nex_rt_argv = argv;
}

long long nex_rt_args_count(void) {
    /* argv[0] is the executable path; user arguments start at 1. */
    return nex_rt_argc > 0 ? (long long)(nex_rt_argc - 1) : 0;
}

static char *nex_rt_copy_string(const char *value) {
    if (value == NULL) {
        value = "";
    }
    size_t length = strlen(value);
    char *copy = (char *)malloc(length + 1);
    if (copy == NULL) {
        return NULL;
    }
    memcpy(copy, value, length + 1);
    return copy;
}

char *nex_rt_arg_at(long long index) {
    if (index < 0 || index >= nex_rt_args_count() || nex_rt_argv == NULL) {
        return nex_rt_copy_string("");
    }
    return nex_rt_copy_string(nex_rt_argv[index + 1]);
}

char *nex_rt_env_get(const char *name) {
    if (name == NULL) {
        return nex_rt_copy_string("");
    }
    return nex_rt_copy_string(getenv(name));
}

void nex_rt_exit(long long code) {
    fflush(stdout);
    fflush(stderr);
    exit((int)code);
}

static void nex_rt_log_line(const char *level, const char *message) {
    char timestamp[32];
    time_t now = time(NULL);
    struct tm parts;
#if defined(_WIN32)
    gmtime_s(&parts, &now);
#else
    gmtime_r(&now, &parts);
#endif
    if (strftime(timestamp, sizeof(timestamp), "%Y-%m-%dT%H:%M:%SZ", &parts) == 0) {
        memcpy(timestamp, "0000-00-00T00:00:00Z", 21);
    }

    while (atomic_flag_test_and_set_explicit(&nex_rt_log_lock, memory_order_acquire)) {
        /* spin */
    }
    fprintf(stderr, "[%s] %-5s %s\n", timestamp, level, message == NULL ? "" : message);
    fflush(stderr);
    atomic_flag_clear_explicit(&nex_rt_log_lock, memory_order_release);
}

void nex_rt_log_info(const char *message) {
    nex_rt_log_line("INFO", message);
}

void nex_rt_log_warn(const char *message) {
    nex_rt_log_line("WARN", message);
}

void nex_rt_log_error(const char *message) {
    nex_rt_log_line("ERROR", message);
}

/* ------------------------------------------------------------------------- */
/* Array collections: in-place ascending sort and string join.               */
/* ------------------------------------------------------------------------- */

static int nex_rt_cmp_i64(const void *left, const void *right) {
    long long a = *(const long long *)left;
    long long b = *(const long long *)right;
    return (a > b) - (a < b);
}

static int nex_rt_cmp_f64(const void *left, const void *right) {
    double a = *(const double *)left;
    double b = *(const double *)right;
    return (a > b) - (a < b);
}

static int nex_rt_cmp_str(const void *left, const void *right) {
    const char *a = *(const char *const *)left;
    const char *b = *(const char *const *)right;
    return strcmp(a == NULL ? "" : a, b == NULL ? "" : b);
}

void nex_rt_sort_i64(void *data, long long length) {
    if (data != NULL && length > 1) {
        qsort(data, (size_t)length, sizeof(long long), nex_rt_cmp_i64);
    }
}

void nex_rt_sort_f64(void *data, long long length) {
    if (data != NULL && length > 1) {
        qsort(data, (size_t)length, sizeof(double), nex_rt_cmp_f64);
    }
}

void nex_rt_sort_str(void *data, long long length) {
    if (data != NULL && length > 1) {
        qsort(data, (size_t)length, sizeof(const char *), nex_rt_cmp_str);
    }
}

char *nex_rt_join_str(void *data, long long length, const char *separator) {
    const char *separator_text = separator == NULL ? "" : separator;
    if (data == NULL || length <= 0) {
        return nex_rt_copy_string("");
    }
    const char **items = (const char **)data;
    size_t separator_length = strlen(separator_text);
    size_t total = 0;
    for (long long index = 0; index < length; index++) {
        const char *item = items[index];
        total += strlen(item == NULL ? "" : item);
    }
    if (separator_length > 0) {
        total += separator_length * (size_t)(length - 1);
    }
    char *joined = (char *)malloc(total + 1);
    if (joined == NULL) {
        return NULL;
    }
    size_t written = 0;
    for (long long index = 0; index < length; index++) {
        const char *item = items[index];
        if (item != NULL) {
            size_t item_length = strlen(item);
            memcpy(joined + written, item, item_length);
            written += item_length;
        }
        if (index + 1 < length && separator_length > 0) {
            memcpy(joined + written, separator_text, separator_length);
            written += separator_length;
        }
    }
    joined[written] = '\0';
    return joined;
}

/* ------------------------------------------------------------------------- */
/* Array reductions: sum, min, max.                                          */
/* ------------------------------------------------------------------------- */

long long nex_rt_array_sum_i64(const void *data, long long length) {
    long long total = 0;
    if (data == NULL || length <= 0) {
        return total;
    }
    const long long *items = (const long long *)data;
    for (long long index = 0; index < length; index++) {
        total += items[index];
    }
    return total;
}

double nex_rt_array_sum_f64(const void *data, long long length) {
    double total = 0.0;
    if (data == NULL || length <= 0) {
        return total;
    }
    const double *items = (const double *)data;
    for (long long index = 0; index < length; index++) {
        total += items[index];
    }
    return total;
}

long long nex_rt_array_min_i64(const void *data, long long length) {
    if (data == NULL || length <= 0) {
        return 0;
    }
    const long long *items = (const long long *)data;
    long long result = items[0];
    for (long long index = 1; index < length; index++) {
        if (items[index] < result) {
            result = items[index];
        }
    }
    return result;
}

long long nex_rt_array_max_i64(const void *data, long long length) {
    if (data == NULL || length <= 0) {
        return 0;
    }
    const long long *items = (const long long *)data;
    long long result = items[0];
    for (long long index = 1; index < length; index++) {
        if (items[index] > result) {
            result = items[index];
        }
    }
    return result;
}

double nex_rt_array_min_f64(const void *data, long long length) {
    if (data == NULL || length <= 0) {
        return 0.0;
    }
    const double *items = (const double *)data;
    double result = items[0];
    for (long long index = 1; index < length; index++) {
        if (items[index] < result) {
            result = items[index];
        }
    }
    return result;
}

double nex_rt_array_max_f64(const void *data, long long length) {
    if (data == NULL || length <= 0) {
        return 0.0;
    }
    const double *items = (const double *)data;
    double result = items[0];
    for (long long index = 1; index < length; index++) {
        if (items[index] > result) {
            result = items[index];
        }
    }
    return result;
}

long long nex_rt_array_count_i64(const void *data, long long length, long long value) {
    long long total = 0;
    if (data == NULL || length <= 0) {
        return total;
    }
    const long long *items = (const long long *)data;
    for (long long index = 0; index < length; index++) {
        if (items[index] == value) {
            total++;
        }
    }
    return total;
}

long long nex_rt_array_count_f64(const void *data, long long length, double value) {
    long long total = 0;
    if (data == NULL || length <= 0) {
        return total;
    }
    const double *items = (const double *)data;
    for (long long index = 0; index < length; index++) {
        if (items[index] == value) {
            total++;
        }
    }
    return total;
}

long long nex_rt_array_count_str(const void *data, long long length, const char *value) {
    long long total = 0;
    if (data == NULL || length <= 0) {
        return total;
    }
    const char *const *items = (const char *const *)data;
    const char *needle = value == NULL ? "" : value;
    for (long long index = 0; index < length; index++) {
        const char *item = items[index] == NULL ? "" : items[index];
        if (strcmp(item, needle) == 0) {
            total++;
        }
    }
    return total;
}

/* ------------------------------------------------------------------------- */
/* Array product.                                                            */
/* ------------------------------------------------------------------------- */

long long nex_rt_array_product_i64(const void *data, long long length) {
    long long result = 1;
    if (data == NULL || length <= 0) {
        return result;
    }
    const long long *items = (const long long *)data;
    for (long long index = 0; index < length; index++) {
        result *= items[index];
    }
    return result;
}

double nex_rt_array_product_f64(const void *data, long long length) {
    double result = 1.0;
    if (data == NULL || length <= 0) {
        return result;
    }
    const double *items = (const double *)data;
    for (long long index = 0; index < length; index++) {
        result *= items[index];
    }
    return result;
}

/* ------------------------------------------------------------------------- */
/* Arena allocators: bump allocator with reset. Handles are heap pointers.   */
/* ------------------------------------------------------------------------- */

typedef struct {
    void *base;
    long long capacity;
    long long used;
} nex_arena;

long long nex_rt_arena_new(long long capacity) {
    if (capacity <= 0) {
        capacity = 4096;
    }
    nex_arena *arena = (nex_arena *)malloc(sizeof(nex_arena));
    if (arena == NULL) {
        return 0;
    }
    arena->base = malloc((size_t)capacity);
    if (arena->base == NULL) {
        free(arena);
        return 0;
    }
    arena->capacity = capacity;
    arena->used = 0;
    return (long long)arena;
}

long long nex_rt_arena_alloc(long long handle, long long size) {
    if (handle == 0 || size <= 0) {
        return 0;
    }
    nex_arena *arena = (nex_arena *)handle;
    /* Align to 8 bytes. */
    long long aligned = (arena->used + 7) & ~7LL;
    if (aligned + size > arena->capacity) {
        long long new_cap = arena->capacity * 2;
        while (new_cap < aligned + size) {
            new_cap *= 2;
        }
        void *grown = realloc(arena->base, (size_t)new_cap);
        if (grown == NULL) {
            return 0;
        }
        arena->base = grown;
        arena->capacity = new_cap;
    }
    void *ptr = (char *)arena->base + aligned;
    arena->used = aligned + size;
    return (long long)ptr;
}

void nex_rt_arena_reset(long long handle) {
    if (handle == 0) {
        return;
    }
    nex_arena *arena = (nex_arena *)handle;
    arena->used = 0;
}

long long nex_rt_arena_used(long long handle) {
    if (handle == 0) {
        return 0;
    }
    return ((nex_arena *)handle)->used;
}

long long nex_rt_arena_capacity(long long handle) {
    if (handle == 0) {
        return 0;
    }
    return ((nex_arena *)handle)->capacity;
}

void nex_rt_arena_free(long long handle) {
    if (handle == 0) {
        return;
    }
    nex_arena *arena = (nex_arena *)handle;
    free(arena->base);
    free(arena);
}

/* ------------------------------------------------------------------------- */
/* String splitting into a growable array of strings.                        */
/* ------------------------------------------------------------------------- */

void *nex_rt_str_split(const char *value, const char *separator) {
    const char *text = value == NULL ? "" : value;
    const char *sep = (separator == NULL || separator[0] == '\0') ? NULL : separator;
    size_t sep_len = sep == NULL ? 0 : strlen(sep);

    /* Count fields. */
    long long count = 1;
    if (sep != NULL) {
        const char *p = text;
        while (1) {
            const char *found = strstr(p, sep);
            if (found == NULL) {
                break;
            }
            count++;
            p = found + sep_len;
        }
    }

    void *header = nex_rt_array_new();
    if (header == NULL) {
        return NULL;
    }
    nex_rt_array_ensure(header, count, (long long)sizeof(char *));
    long long *words = (long long *)header;
    void **data = (void **)header;
    words[1] = count;
    if (words[2] < count) {
        words[2] = count;
    }

    char **items = (char **)data[0];
    if (items == NULL && count > 0) {
        /* ensure data buffer */
        data[0] = malloc((size_t)count * sizeof(char *));
        items = (char **)data[0];
    }

    if (sep == NULL) {
        items[0] = nex_rt_copy_string(text);
        for (long long i = 1; i < count; i++) {
            items[i] = nex_rt_copy_string("");
        }
        return header;
    }

    const char *p = text;
    for (long long i = 0; i < count; i++) {
        const char *found = strstr(p, sep);
        size_t field_len;
        if (found == NULL) {
            field_len = strlen(p);
        } else {
            field_len = (size_t)(found - p);
        }
        char *field = (char *)malloc(field_len + 1);
        if (field != NULL) {
            memcpy(field, p, field_len);
            field[field_len] = '\0';
        } else {
            field = nex_rt_copy_string("");
        }
        items[i] = field;
        if (found != NULL) {
            p = found + sep_len;
        } else {
            p = text + strlen(text);
        }
    }
    return header;
}

/* ------------------------------------------------------------------------- */
/* Filesystem helpers.                                                      */
/* ------------------------------------------------------------------------- */

int nex_rt_file_append(const char *path, const char *content) {
    if (path == NULL || content == NULL) {
        return 0;
    }
    FILE *file = fopen(path, "ab");
    if (file == NULL) {
        return 0;
    }
    size_t length = strlen(content);
    size_t written = fwrite(content, 1, length, file);
    fclose(file);
    return written == length;
}

int nex_rt_file_delete(const char *path) {
    if (path == NULL) {
        return 0;
    }
    return remove(path) == 0;
}

int nex_rt_file_rename(const char *from, const char *to) {
    if (from == NULL || to == NULL) {
        return 0;
    }
    return rename(from, to) == 0;
}

int nex_rt_file_copy(const char *from, const char *to) {
    if (from == NULL || to == NULL) {
        return 0;
    }
    FILE *src = fopen(from, "rb");
    if (src == NULL) {
        return 0;
    }
    FILE *dst = fopen(to, "wb");
    if (dst == NULL) {
        fclose(src);
        return 0;
    }
    char buffer[4096];
    size_t read;
    int ok = 1;
    while ((read = fread(buffer, 1, sizeof(buffer), src)) > 0) {
        if (fwrite(buffer, 1, read, dst) != read) {
            ok = 0;
            break;
        }
    }
    if (ferror(src)) {
        ok = 0;
    }
    fclose(src);
    fclose(dst);
    if (!ok) {
        remove(to);
    }
    return ok;
}

/* ------------------------------------------------------------------------- */
/* Array slicing: returns a new growable array (copy of a sub-range).       */
/* ------------------------------------------------------------------------- */

void *nex_rt_array_slice(const void *data, long long len, long long elem_size, long long start, long long end) {
    if (data == NULL || len <= 0) {
        return nex_rt_array_new();
    }
    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start > end) { start = end; }
    long long count = end - start;
    if (count < 0) count = 0;
    if (elem_size <= 0) elem_size = 1;

    void *header = nex_rt_array_new();
    if (header == NULL) {
        return NULL;
    }
    nex_rt_array_ensure(header, count, elem_size);
    long long *words = (long long *)header;
    void **data_ptr = (void **)header;
    words[1] = count;
    if (words[2] < count) words[2] = count;
    if (count > 0 && data_ptr[0] != NULL) {
        memcpy(data_ptr[0], (const char *)data + start * elem_size, (size_t)(count * elem_size));
    }
    return header;
}

/* ------------------------------------------------------------------------- */
/* Explicit deep copy (clone). `string` and `[T]` are move-only; `clone`      */
/* produces an independent copy so value semantics stay available on demand.  */
/* ------------------------------------------------------------------------- */

char *nex_rt_str_clone(const char *value) {
    if (value == NULL) {
        return NULL;
    }
    size_t length = strlen(value);
    char *result = (char *)malloc(length + 1);
    if (result == NULL) {
        return NULL;
    }
    memcpy(result, value, length + 1);
    return result;
}

void *nex_rt_array_clone(void *header, long long elem_size) {
    if (header == NULL) {
        return nex_rt_array_new();
    }
    /* The growable-array header is `{ data, len, cap }`; `nex_rt_array_slice`
       expects the element buffer, not the header, so pass `data`. Passing the
       header here copied the header bytes themselves into the new array, which
       is how a cloned struct field ended up holding pointer bits instead of
       elements. */
    void **data_ptr = (void **)header;
    long long len = ((long long *)header)[1];
    if (len <= 0 || data_ptr[0] == NULL) {
        return nex_rt_array_new();
    }
    return nex_rt_array_slice(data_ptr[0], len, elem_size, 0, len);
}

/* Frees a growable array: its backing data and the header. Safe on NULL. */
void nex_rt_array_free(void *header) {
    if (header == NULL) {
        return;
    }
    void **data = (void **)header;
    free(data[0]);
    free(header);
}
