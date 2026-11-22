import unittest

import mpmath as mp

import reference as ref


class ReferenceTests(unittest.TestCase):
    def test_midpoint_uses_nearest_even(self):
        true = 1 + mp.mpf(2) ** -53
        adjacent = 1 + 2 ** -52
        self.assertEqual(ref.numeric_class(true, 1.0, adjacent)[0], 'ours-wrong')
        self.assertEqual(ref.numeric_class(true, adjacent, 1.0)[0], 'excel-quirk')

    def test_spacing_uses_true_binade_below_power_of_two(self):
        true = 1 - mp.mpf(2) ** -55
        self.assertEqual(ref.ulps_from(true, 1.0), mp.mpf('0.25'))

    def test_zero_and_subnormals_have_defined_spacing(self):
        smallest = 2.0 ** -1074
        self.assertEqual(ref.ulps_from(0, smallest), 1)
        self.assertEqual(ref.numeric_class(0, 0.0, smallest)[0], 'ours-wrong')

    def test_rounding_subnormal_avoids_double_rounding(self):
        true = mp.ldexp(mp.mpf('0.5000000000000000001'), -1074)
        self.assertEqual(ref.numeric_class(true, 2.0 ** -1074, 0.0)[0], 'ours-wrong')

    def test_small_beta_cdf_uses_the_small_tail(self):
        x = mp.mpf(2) ** -300
        self.assertEqual(ref.beta_lower(1, 1, x), x)
        self.assertTrue(mp.almosteq(ref.beta_lower(2, 3, mp.mpf('0.75')),
                                       mp.mpf('0.94921875')))

    def test_reflected_beta_keeps_a_tiny_cdf(self):
        x = 1 - mp.ldexp(1, -52)
        shape = mp.ldexp(1, -300)
        expected = -mp.expm1(shape * mp.log1p(-x) + mp.log1p(shape * x))
        actual = ref.beta_lower(2, shape, x)
        self.assertTrue(mp.almosteq(actual, expected, rel_eps=mp.mpf('1e-50'), abs_eps=0))

    def test_rate_solves_the_balance_equation(self):
        ref.X0 = 0.99
        self.assertEqual(float(ref.R['RATE'](2, 0, -1, 4)), 1)

    def test_rate_proves_a_balance_has_no_real_root(self):
        with self.assertRaises(ref.NoRealReference):
            ref.R['RATE'](-0.01, 0, -100, -1)
        with self.assertRaises(ref.NoRealReference):
            ref.R['RATE'](-0.01, -1e-30, 100, -1e-50, 1)

    def test_mirr_discounts_financed_and_reinvested_cash_flows(self):
        result = ref.R['MIRR']([-100.0, 0.0, 121.0], 0.05, 0.08)
        self.assertTrue(mp.almosteq(result, mp.mpf('0.1')))


if __name__ == '__main__':
    unittest.main()
