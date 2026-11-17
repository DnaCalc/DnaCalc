// predict NORM.S.INV for central piece A inputs p (binary file of doubles) with coefficient file; output doubles (NaN if not piece A)
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
static inline double st(long double v){return (double)v;}
static const double SQ2=1.4142135623730951;
int main(int argc,char**argv){
  double C[16]; FILE*fc=fopen(argv[1],"r"); for(int i=0;i<16;i++) if(fscanf(fc,"%la",&C[i])!=1) return 1;
  FILE*fi=fopen(argv[2],"rb"); fseek(fi,0,SEEK_END); long n=ftell(fi)/8; rewind(fi);
  double*p=malloc(n*8),*o=malloc(n*8); if(fread(p,8,n,fi)!=n) return 2;
  for(long i=0;i<n;i++){
    double q=p[i]-0.5; double t=2*q; double s=st((long double)t*t);
    if(q< -0.425 || p[i]>15.0/16 || s>0.5 || t==0){o[i]=NAN;continue;}
    double P=C[7]; for(int k=6;k>=0;k--) P=st((long double)st((long double)P*s)+C[k]);
    double Q=C[15]; for(int k=6;k>=0;k--) Q=st((long double)st((long double)Q*s)+C[8+k]);
    double R=st((long double)P/Q); double y=st((long double)t*R); o[i]=st((long double)y*SQ2);
  }
  FILE*fo=fopen(argv[3],"wb"); fwrite(o,8,n,fo); fclose(fo);
}
