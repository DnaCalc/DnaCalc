// Rational minimax (relative error) of f on [A,B]: P(t)/Q(t), deg m/n, t = x - SHIFT, Q monic (q_n = 1)
// or Q0 = 1 (NORM=0). __float128 throughout. Prints coefficients as hex doubles and decimal.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <quadmath.h>
typedef __float128 Q;
static int M,N,NORM,FUNC; static Q A,B,SH;
static Q fun(Q x){ if(FUNC==0) return tgammaq(x); if(FUNC==1){ if(fabsq(x-2)<1e-25Q) return 1-0.57721566490153286060651209008240243Q; return lgammaq(x)/(x-2);} if(FUNC==2){ if(fabsq(x-1)<1e-25Q) return -0.57721566490153286060651209008240243Q; return lgammaq(x)/(x-1);} return 0;}
static void solve(int n, Q *a, Q *b){
  for(int i=0;i<n;i++){int p=i;for(int r=i+1;r<n;r++) if(fabsq(a[r*n+i])>fabsq(a[p*n+i]))p=r;
    if(p!=i){for(int c=0;c<n;c++){Q t=a[i*n+c];a[i*n+c]=a[p*n+c];a[p*n+c]=t;}Q t=b[i];b[i]=b[p];b[p]=t;}
    for(int r=i+1;r<n;r++){Q f=a[r*n+i]/a[i*n+i];for(int c=i;c<n;c++)a[r*n+c]-=f*a[i*n+c];b[r]-=f*b[i];}}
  for(int i=n-1;i>=0;i--){Q s=b[i];for(int c=i+1;c<n;c++)s-=a[i*n+c]*b[c];b[i]=s/a[i*n+i];}
}
static Q p[40],q[40];
static Q pv(Q t){Q s=0;for(int k=M;k>=0;k--)s=s*t+p[k];return s;}
static Q qv(Q t){Q s=0;for(int k=N;k>=0;k--)s=s*t+q[k];return s;}
static Q err(Q x){Q t=x-SH; if(getenv("ABS")) return pv(t)/qv(t)-fun(x); return pv(t)/qv(t)/fun(x)-1;}
int main(int argc,char**argv){
  M=atoi(argv[1]);N=atoi(argv[2]);A=strtoflt128(argv[3],0);B=strtoflt128(argv[4],0);NORM=atoi(argv[5]);SH=strtoflt128(argv[6],0);FUNC=atoi(argv[7]);
  int R=M+N+2; Q *x=malloc(R*sizeof(Q));
  for(int i=0;i<R;i++) x[i]=(A+B)/2-(B-A)/2*cosq(M_PIq*i/(R-1));
  for(int k=0;k<=N;k++)q[k]=0; if(NORM) q[N]=1; else q[0]=1;
  // initial Q_old = 1
  Q *qold=malloc(R*sizeof(Q)); for(int i=0;i<R;i++)qold[i]=1;
  Q E=0;
  for(int outer=0;outer<60;outer++){
    for(int inner=0;inner<30;inner++){
      int U=R; Q *a=calloc(U*U,sizeof(Q)),*b=calloc(U,sizeof(Q));
      for(int i=0;i<R;i++){Q t=x[i]-SH,f=fun(x[i]); int c=0; Q pw=1;
        for(int k=0;k<=M;k++){a[i*U+c++]=pw;pw*=t;}
        pw=1; Q fixed=0; for(int k=0;k<=N;k++){int fx=NORM?(k==N):(k==0); if(fx) fixed=pw; else a[i*U+c++]=-f*pw; pw*=t;}
        a[i*U+c]= -((i&1)?-1:1)*(getenv("ABS")?1:f)*qold[i]; b[i]=f*fixed;}
      solve(U,a,b);
      int c=0; for(int k=0;k<=M;k++)p[k]=b[c++];
      for(int k=0;k<=N;k++){int fx=NORM?(k==N):(k==0); q[k]= fx?1:b[c++];}
      Q En=b[c]; free(a);free(b);
      for(int i=0;i<R;i++)qold[i]=qv(x[i]-SH);
      if(fabsq(En-E)<=fabsq(En)*1e-25Q){E=En;break;} E=En;
    }
    // exchange: find extrema of err between sign changes on fine grid
    int G=20000; Q *gx=malloc((G+1)*sizeof(Q)),*ge=malloc((G+1)*sizeof(Q));
    for(int i=0;i<=G;i++){gx[i]=A+(B-A)*i/G; ge[i]=err(gx[i]);}
    // segments by sign
    Q nx[100]; int nn=0; int s=0;
    int start=0;
    for(int i=1;i<=G+1;i++){ if(i==G+1 || (ge[i]>0)!=(ge[start]>0)){ // segment start..i-1
        int best=start; for(int j=start;j<i;j++) if(fabsq(ge[j])>fabsq(ge[best]))best=j;
        // refine by golden section on neighbors
        Q lo=gx[best>0?best-1:0], hi=gx[best<G?best+1:G];
        for(int it=0;it<200;it++){Q m1=lo+(hi-lo)/3,m2=hi-(hi-lo)/3; if(fabsq(err(m1))>fabsq(err(m2)))hi=m2; else lo=m1;}
        Q xm=(lo+hi)/2; if(best==0)xm=A; if(best==G)xm=B;
        if(nn<100) nx[nn++]=xm; start=i;}}
    Q maxe=0; for(int i=0;i<=G;i++) if(fabsq(ge[i])>maxe)maxe=fabsq(ge[i]);
    free(gx);free(ge);
    if(nn!=R){ // keep R largest by dropping smallest at ends
      while(nn>R){ if(fabsq(err(nx[0]))<fabsq(err(nx[nn-1]))){memmove(nx,nx+1,(nn-1)*sizeof(Q));} nn--; }
    }
    if(nn<R){fprintf(stderr,"outer %d: only %d extrema, E=%.3e\n",outer,nn,(double)E); }
    else for(int i=0;i<R;i++)x[i]=nx[i];
    fprintf(stderr,"outer %d E=%.6e max=%.6e\n",outer,(double)E,(double)maxe);
    if(fabsq(maxe-fabsq(E))<1e-6Q*fabsq(E)) break;
  }
  for(int k=0;k<=M;k++){double d=(double)p[k];uint64_t u;memcpy(&u,&d,8);char buf[64];quadmath_snprintf(buf,64,"%.30Qe",p[k]);printf("P%d %016lx %s\n",k,u,buf);}
  for(int k=0;k<=N;k++){double d=(double)q[k];uint64_t u;memcpy(&u,&d,8);char buf[64];quadmath_snprintf(buf,64,"%.30Qe",q[k]);printf("Q%d %016lx %s\n",k,u,buf);}
}
