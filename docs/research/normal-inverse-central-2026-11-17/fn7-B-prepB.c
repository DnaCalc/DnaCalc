// piece B/C: from p and x, |t|, true y=erfinv(|t|) (binary128), y candidates with st(y*SQ2)=|x|
#include <quadmath.h>
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
static inline double st(long double v){return (double)v;}
static const double SQ2=1.4142135623730951;
__float128 erfinvq(__float128 t){ double td=(double)t; double yd=td; for(int i=0;i<80;i++){double e=erf(yd)-td; yd-=e/(1.1283791670955126*exp(-yd*yd)); if(fabs(e)<1e-17)break;} __float128 y=yd; for(int i=0;i<4;i++){ __float128 e=erfq(y)-t; y-=e/(M_2_SQRTPIq*expq(-y*y)); } return y; }
int main(int argc,char**argv){
  FILE*fp=fopen(argv[1],"rb"),*fx=fopen(argv[2],"rb"); fseek(fp,0,SEEK_END); long n=ftell(fp)/8; rewind(fp);
  double*p=malloc(n*8),*x=malloc(n*8); if(fread(p,8,n,fp)!=n||fread(x,8,n,fx)!=n) return 2;
  double*o=malloc(n*8*6);
  #pragma omp parallel for schedule(dynamic,1024)
  for(long i=0;i<n;i++){ double q=p[i]-0.5, t=fabs(2*q), ax=fabs(x[i]); __float128 Y=erfinvq(t); double hi=(double)Y, lo=(double)(Y-hi);
    double c[3]; int k=0; double y0=ax/SQ2; for(int a=-4;a<=4;a++){ double y=y0; for(int j=0;j<abs(a);j++) y=nextafter(y,a>0?INFINITY:-INFINITY); if(st((long double)y*SQ2)==ax && k<3){int d=0; for(int m=0;m<k;m++) if(c[m]==y) d=1; if(!d) c[k++]=y;} }
    double*r=o+6*i; r[0]=t; r[1]=hi; r[2]=lo; r[3]=k; r[4]=k>0?c[0]:NAN; r[5]=k>1?c[1]:NAN; }
  FILE*fo=fopen(argv[3],"wb"); fwrite(o,8,6*n,fo); fclose(fo);
}
