#ifndef LM_RESIZER_H
#define LM_RESIZER_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define LM_RESIZER_ABI_VERSION 1

char *lm_resizer_compress_json(
    const unsigned char *content_ptr,
    size_t content_len,
    const unsigned char *query_ptr,
    size_t query_len
);

void lm_resizer_string_free(char *ptr);

unsigned char *lm_resizer_alloc(size_t len);
void lm_resizer_free(unsigned char *ptr, size_t len);

#ifdef __cplusplus
}
#endif

#endif
