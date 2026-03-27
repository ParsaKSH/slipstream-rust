#include <stddef.h>
#include "picotls.h"

/* MSVC uses static_assert instead of _Static_assert */
#ifdef _MSC_VER
#define LAYOUT_ASSERT_EQ(a, b) static_assert((a) == (b), "picotls layout mismatch")
#else
#define LAYOUT_ASSERT_EQ(a, b) _Static_assert((a) == (b), "picotls layout mismatch")
#endif

LAYOUT_ASSERT_EQ(offsetof(ptls_iovec_t, base), 0);
LAYOUT_ASSERT_EQ(offsetof(ptls_iovec_t, len), sizeof(void *));
