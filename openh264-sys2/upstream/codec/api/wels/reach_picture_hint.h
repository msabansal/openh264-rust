#ifndef REACH_PICTURE_HINT_H
#define REACH_PICTURE_HINT_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
  REACH_ROLE_UNSPECIFIED = 0,
  REACH_ROLE_MAIN = 1,
  REACH_ROLE_AUXILIARY = 2
} EReachPictureRole;

#define REACH_HINT_ROLE_REFERENCE 0x1 /* predict from previous same-role picture */
#define REACH_HINT_RECOVERY_16X16 0x2 /* startup/recovery aux: 16x16-only search */
#define REACH_HINT_STATIC_SKIP    0x4 /* skip search for unchanged macroblocks */

typedef struct TagReachPictureHint {
  int32_t iRole;            /* EReachPictureRole */
  int32_t bRecovery;        /* 1 = aux following a main IDR (startup/forced) */
  int32_t iFlags;           /* REACH_HINT_* */
  const uint8_t* pStaticMb; /* optional raster map, 1 byte per 16x16 coded MB;
                               nonzero = source MB identical to the previous
                               encoded picture of the same role */
  int32_t iStaticMbCount;   /* == (coded_w/16)*(coded_h/16) when map != NULL */
} SReachPictureHint;

/* Applies to the NEXT EncodeFrame on this encoder only, then reverts to stock.
   Copies pStaticMb into encoder-owned storage. pHint == NULL clears.
   Returns 0 on success; nonzero for unknown flag bits/role, bad map size, or a
   flag that requires a single-slice single-thread encoder when that is not
   the case. */
int ReachSetPictureHint(void* pSvcEncoder /* ISVCEncoder* */,
                        const SReachPictureHint* pHint);

#ifdef __cplusplus
}
#endif
#endif
