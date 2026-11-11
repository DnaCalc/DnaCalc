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
  ld n = chain(P,x,mode&1), d = chain(Q,x,mode&1);
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
