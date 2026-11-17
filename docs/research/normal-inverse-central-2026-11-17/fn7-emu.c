// emulate candidate: s=st(t*t); P,Q Horner with each op stored (x87 64-bit then double); R=st(P/Q); y=st(t*R); x=st(y*SQ2)
// usage: emu coeffs.txt (15 P coeffs p0..p7? we use: line1: 8 P coeffs p0..p7, line2: 8 Q coeffs q0..q7, hex or decimal)
// reads t.bin x.bin ; writes match stats per s-band and optionally emu_x.bin
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <string.h>
static inline double st(long double v){return (double)v;}
static const double SQ2=1.4142135623730951;
int main(int argc,char**argv){
  double P[8],Q[8]; FILE*fc=fopen(argv[1],"r");
  for(int i=0;i<8;i++) if(fscanf(fc,"%la",&P[i])!=1) return 1;
  for(int i=0;i<8;i++) if(fscanf(fc,"%la",&Q[i])!=1) return 1;
  int mode = argc>2? atoi(argv[2]):0; // 0: all stores; 1: no stores inside Horner (extended), store P,Q
  FILE*ft=fopen("t.bin","rb"),*fx=fopen("x.bin","rb");
  fseek(ft,0,SEEK_END); long n=ftell(ft)/8; rewind(ft);
  double*t=malloc(n*8),*x=malloc(n*8),*ex=malloc(n*8);
  if(fread(t,8,n,ft)!=n||fread(x,8,n,fx)!=n) return 2;
  long hit[20]={0},tot[20]={0};
  for(long i=0;i<n;i++){
    double s=st((long double)t[i]*t[i]); if(s>0.5){ex[i]=NAN;continue;}
    double p,q;
    if(mode==0){
      p=P[7]; for(int k=6;k>=0;k--) p=st((long double)st((long double)p*s)+P[k]);
      q=Q[7]; for(int k=6;k>=0;k--) q=st((long double)st((long double)q*s)+Q[k]);
    } else {
      long double lp=P[7],lq=Q[7]; for(int k=6;k>=0;k--){lp=lp*s+P[k]; lq=lq*s+Q[k];} p=st(lp); q=st(lq);
    }
    double R=st((long double)p/q); double y=st((long double)t[i]*R); double xx=st((long double)y*SQ2);
    ex[i]=xx;
    int b = s<1e-6?0: s<1e-3?1: s<1e-2?2: 3+(int)(s*30); if(b>19)b=19;
    tot[b]++; hit[b]+= (xx==x[i]);
  }
  long H=0,N=0;
  for(int b=0;b<20;b++) if(tot[b]){ printf("band %2d n=%7ld match=%.4f\n",b,tot[b],(double)hit[b]/tot[b]); H+=hit[b];N+=tot[b];}
  printf("total %ld/%ld = %.4f\n",H,N,(double)H/N);
  if(argc>3){FILE*fo=fopen(argv[3],"wb"); fwrite(ex,8,n,fo); fclose(fo);}
}
