// For integer a rows: implied Excel exponent of e^-x x^a / G  under normalisations: N0: none (a ln x - x), N1: /Gamma(a), N2: /Gamma(a+1), N3: Stirling t1 (rt2pin sqrt a)
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <quadmath.h>
static double hd(uint64_t u){double d;memcpy(&d,&u,8);return d;}
static uint64_t bd(double d){uint64_t u;memcpy(&u,&d,8);return u;}
int main(int c,char**v){FILE*F=fopen(v[1],"r");uint64_t a,x,e;
 while(fscanf(F,"%lx %lx %lx",&a,&x,&e)==3){double A=hd(a),X=hd(x),E=hd(e); __float128 s=0,t=1; int na=(int)A;
  for(int j=0;j<na;j++){ if(j) t=t*X/j; s+=t; }
  __float128 qt=expq(-(__float128)X)*s; __float128 dl=logq((__float128)E/qt);
  __float128 T0=A*logq((__float128)X)-X+dl; __float128 L[4]={T0, T0-lgammaq((__float128)A), T0-lgammaq((__float128)A+1), T0-lgammaq((__float128)A)-logq(sqrtq((__float128)A)/sqrtq(2*M_PIq))};
  printf("%016lx %016lx %016lx",a,x,e); for(int i=0;i<4;i++){double hi=(double)L[i]; double lo=(double)(L[i]-hi); printf(" %016lx %016lx",bd(hi),bd(lo));} printf("\n");}}
