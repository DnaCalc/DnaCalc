import remez as R
from remez import mp
W={'rel':lambda x:R.Rf(x),
   'abs_erfinv':lambda x:R.Rf(x)*mp.sqrt(x) if x>0 else mp.mpf('1e-30'),
   'absR':lambda x:mp.mpf(1),
   'rel_sqrt1ms':lambda x:R.Rf(x)*mp.sqrt(1-x),
   'rel_1ms':lambda x:R.Rf(x)*(1-x)}
