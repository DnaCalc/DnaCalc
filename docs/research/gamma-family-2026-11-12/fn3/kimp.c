#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <quadmath.h>
static double hd(uint64_t u){double d;memcpy(&d,&u,8);return d;}
int main(int c,char**v){FILE*F=fopen(v[1],"r");uint64_t a,x,e,t,ex,s; int hist[41]={0}, hist2[41]={0},n=0;
 while(fscanf(F,"%lx %lx %lx %lx %lx %lx",&a,&x,&e,&t,&ex,&s)==6){double A=hd(a),X=hd(x),E=hd(e),T=hd(t),EX=hd(ex);
  __float128 S=0,tt=1; int na=(int)A; for(int j=0;j<na;j++){ if(j) tt=tt*X/j; S+=tt; }
  // exact K: Q = exp(T) * K where K = sqrt(a/2pi)/x * S * (a-1)!... careful: Q_true = e^-x sum x^j/j!; mass(a)=e^-x x^a/a! = exp(Eexact)/sqrt(2 pi a)
  // Q/exp(Eexact) = Q/(mass*sqrt(2 pi a)); K = Q_true/(mass_true*sqrt(2pi a))
  __float128 mass=expq(-(__float128)X+A*logq((__float128)X)-lgammaq((__float128)A+1));
  __float128 K=expq(-(__float128)X)*S/(mass*sqrtq(2*M_PIq*A));
  __float128 KE=(__float128)E/expq((__float128)T);        // implied with exact exp of Excel exponent
  __float128 KE2=(__float128)E/(__float128)EX;            // implied with our stored EXP
  int ek; frexpq(K,&ek); __float128 ulpk=ldexpq(1.0Q,ek-53);
  int d=(int)lrint((double)((KE-K)/ulpk)); int d2=(int)lrint((double)((KE2-K)/ulpk));
  if(d<-20)d=-20; if(d>20)d=20; if(d2<-20)d2=-20; if(d2>20)d2=20; hist[d+20]++; hist2[d2+20]++; n++;}
 printf("n=%d\nexact-exp:",n); for(int i=0;i<41;i++) if(hist[i]) printf(" %d:%d",i-20,hist[i]); printf("\nstored-exp:"); for(int i=0;i<41;i++) if(hist2[i]) printf(" %d:%d",i-20,hist2[i]); printf("\n");}
