#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <ctype.h>
#include <time.h>

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

long long nex_rt_char_code(const char *value) {
    if (value == NULL || value[0] == '\0') {
        return -1;
    }
    return (long long)(unsigned char)value[0];
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
    long long new_cap = cap > 0 ? cap * 2 : 4;
    while (new_cap < needed_len) {
        new_cap *= 2;
    }
    long long bytes = new_cap * (elem_size > 0 ? elem_size : 1);
    void *grown = realloc(data[0], (size_t)bytes);
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