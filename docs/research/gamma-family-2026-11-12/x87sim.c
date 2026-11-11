/* x87 PC64 emulation via long double on x86-64 Linux (default control word: extended, RN). */
#include <stdint.h>
typedef long double ld;
static inline double st(ld v){ volatile double d = (double)v; return d; }
/* B3 graph: (x-2) * st(P/Q), stored Horner in x, last sum of each polynomial resident.
   mode bit0: store the last sums too; bit1: keep r resident into the final multiply. */
static ld chain(const double *c, double x, int storelast){
  double v = c[8];
  for(int k=7;k>=1;k--){ double p = st((ld)v*(ld)x); v = st((ld)p + (ld)c[k]); }
  double p = st((ld)v*(ld)x);
  ld s = (ld)p + (ld)c[0];
  if(storelast) s = (ld)st(s);
  return s;
}
double b3(const double *P, const double *Q, double x, int mode){
  ld n = chain(P,x,(mode&1)||(mode&4)), d = chain(Q,x,(mode&1)||(mode&8));
  ld r = n/d;
  if(!(mode&2)) r = (ld)st(r);
  return st(r*(ld)(x-2.0));
}
void b3_batch(const double *P, const double *Q, const double *xs, double *out, int n, int mode){
  for(int i=0;i<n;i++) out[i]=b3(P,Q,xs[i],mode);
}
/* real-valued (long double, no stores) P/Q*(x-2) for bias reference */
void b3_real_batch(const double *P, const double *Q, const double *xs, double *out, int n){
  for(int i=0;i<n;i++){ ld x=xs[i], a=P[8], b=Q[8]; for(int k=7;k>=0;k--){a=a*x+P[k]; b=b*x+Q[k];} out[i]=(double)(a/b*(x-2)); }
}
#include <fenv.h>
/* stage-mode variant: rm[s] for each stored stage s (0 RN, else fe rounding constant applied to the store).
   stage index: P: 0..14 (prod8,sum7,prod7,sum6,...,sum1,prod1), Q: 15..29, R: 30, final: 31; 32/33: P/Q last sums (store if rm>=0 nonzero) */
static inline double stm(ld v, int m){ if(!m){ volatile double d=(double)v; return d;} int o=fegetround(); fesetround(m); volatile double d=(double)v; fesetround(o); return d; }
static ld chainm(const double *c, double x, const int *rm, int base, int lastm){
  double v=c[8]; int s=base;
  for(int k=7;k>=1;k--){ double p=stm((ld)v*(ld)x, rm[s++]); v=stm((ld)p+(ld)c[k], rm[s++]); }
  double p=stm((ld)v*(ld)x, rm[s++]);
  ld sum=(ld)p+(ld)c[0];
  if(lastm>=0) sum=(ld)stm(sum,lastm);
  return sum;
}
void b3m_batch(const double *P, const double *Q, const double *xs, double *out, int n, const int *rm){
  for(int i=0;i<n;i++){ double x=xs[i];
    ld nu=chainm(P,x,rm,0,rm[32]), de=chainm(Q,x,rm,15,rm[33]);
    ld r=nu/de; if(rm[30]>=0) r=(ld)stm(r,rm[30]);
    out[i]=stm(r*(ld)(x-2.0), rm[31]); }
}
