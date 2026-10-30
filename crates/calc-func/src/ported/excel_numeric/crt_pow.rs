//! The Windows runtime's `pow` in portable Rust (dc-sbo): Arm's optimized-routines `pow`
//! (Szabolcs Nagy; `math/pow.c` and `math/pow_log_data.c` of
//! https://github.com/ARM-software/optimized-routines at 0b5e3775, MIT OR Apache-2.0 WITH
//! LLVM-exception, used under MIT; Copyright (c) 2018-2026 Arm Limited; notice in
//! `deploy/notices/ported.txt`) on its fused multiply-add path, with Arm's 256-entry exp
//! configuration (`EXP_TABLE_BITS 8`, the order-5 wide polynomial, `EXP_USE_TOINT_NARROW`) and
//! both tables from Arm's sources. That this is what the x64 runtime's `pow` gives on a CPU with
//! FMA3 is measured, by calling the runtime: the port equals it on 8,000,000 arguments, where
//! the same code with Arm's default 128-entry configuration differs on up to 773 of 1,000,000.
//!
//! `f64::mul_add` is the one fused multiply-add: the instruction where the target has it, a
//! correctly rounded software form elsewhere, so the bits do not depend on the target.


const POW_LOG_TABLE_BITS: u32 = 7;
const N_LOG: u64 = 1 << POW_LOG_TABLE_BITS;
const OFF: u64 = 0x3fe6_9555_0000_0000;
const LN2HI: f64 = f64::from_bits(0x3fe62e42fefa3800);
const LN2LO: f64 = f64::from_bits(0x3d2ef35793c76730);
/// `__pow_log_data.poly`, scaled as the evaluation uses it.
const A: [f64; 7] = [
    f64::from_bits(0xbfe0000000000000),
    f64::from_bits(0xbfe5555555555560),
    f64::from_bits(0x3fe0000000000006),
    f64::from_bits(0x3fe999999959554e),
    f64::from_bits(0xbfe555555529a47a),
    f64::from_bits(0xbff2495b9b4845e9),
    f64::from_bits(0x3ff0002b8b263fc3),
];
/// `__pow_log_data.tab`: (invc, logc, logctail) as bit patterns.
#[rustfmt::skip]
const T: [(u64, u64, u64); 128] = [
    (0x3ff6a00000000000, 0xbfd62c82f2b9c800, 0x3cfab42428375680),
    (0x3ff6800000000000, 0xbfd5d1bdbf580800, 0xbd1ca508d8e0f720),
    (0x3ff6600000000000, 0xbfd5767717455800, 0xbd2362a4d5b6506d),
    (0x3ff6400000000000, 0xbfd51aad872df800, 0xbce684e49eb067d5),
    (0x3ff6200000000000, 0xbfd4be5f95777800, 0xbd041b6993293ee0),
    (0x3ff6000000000000, 0xbfd4618bc21c6000, 0x3d13d82f484c84cc),
    (0x3ff5e00000000000, 0xbfd404308686a800, 0x3cdc42f3ed820b3a),
    (0x3ff5c00000000000, 0xbfd3a64c55694800, 0x3d20b1c686519460),
    (0x3ff5a00000000000, 0xbfd347dd9a988000, 0x3d25594dd4c58092),
    (0x3ff5800000000000, 0xbfd2e8e2bae12000, 0x3d267b1e99b72bd8),
    (0x3ff5600000000000, 0xbfd2895a13de8800, 0x3d15ca14b6cfb03f),
    (0x3ff5600000000000, 0xbfd2895a13de8800, 0x3d15ca14b6cfb03f),
    (0x3ff5400000000000, 0xbfd22941fbcf7800, 0xbd165a242853da76),
    (0x3ff5200000000000, 0xbfd1c898c1699800, 0xbd1fafbc68e75404),
    (0x3ff5000000000000, 0xbfd1675cababa800, 0x3d1f1fc63382a8f0),
    (0x3ff4e00000000000, 0xbfd1058bf9ae4800, 0xbd26a8c4fd055a66),
    (0x3ff4c00000000000, 0xbfd0a324e2739000, 0xbd0c6bee7ef4030e),
    (0x3ff4a00000000000, 0xbfd0402594b4d000, 0xbcf036b89ef42d7f),
    (0x3ff4a00000000000, 0xbfd0402594b4d000, 0xbcf036b89ef42d7f),
    (0x3ff4800000000000, 0xbfcfb9186d5e4000, 0x3d0d572aab993c87),
    (0x3ff4600000000000, 0xbfcef0adcbdc6000, 0x3d2b26b79c86af24),
    (0x3ff4400000000000, 0xbfce27076e2af000, 0xbd172f4f543fff10),
    (0x3ff4200000000000, 0xbfcd5c216b4fc000, 0x3d21ba91bbca681b),
    (0x3ff4000000000000, 0xbfcc8ff7c79aa000, 0x3d27794f689f8434),
    (0x3ff4000000000000, 0xbfcc8ff7c79aa000, 0x3d27794f689f8434),
    (0x3ff3e00000000000, 0xbfcbc286742d9000, 0x3d194eb0318bb78f),
    (0x3ff3c00000000000, 0xbfcaf3c94e80c000, 0x3cba4e633fcd9066),
    (0x3ff3a00000000000, 0xbfca23bc1fe2b000, 0xbd258c64dc46c1ea),
    (0x3ff3a00000000000, 0xbfca23bc1fe2b000, 0xbd258c64dc46c1ea),
    (0x3ff3800000000000, 0xbfc9525a9cf45000, 0xbd2ad1d904c1d4e3),
    (0x3ff3600000000000, 0xbfc87fa06520d000, 0x3d2bbdbf7fdbfa09),
    (0x3ff3400000000000, 0xbfc7ab890210e000, 0x3d2bdb9072534a58),
    (0x3ff3400000000000, 0xbfc7ab890210e000, 0x3d2bdb9072534a58),
    (0x3ff3200000000000, 0xbfc6d60fe719d000, 0xbd10e46aa3b2e266),
    (0x3ff3000000000000, 0xbfc5ff3070a79000, 0xbd1e9e439f105039),
    (0x3ff3000000000000, 0xbfc5ff3070a79000, 0xbd1e9e439f105039),
    (0x3ff2e00000000000, 0xbfc526e5e3a1b000, 0xbd20de8b90075b8f),
    (0x3ff2c00000000000, 0xbfc44d2b6ccb8000, 0x3d170cc16135783c),
    (0x3ff2c00000000000, 0xbfc44d2b6ccb8000, 0x3d170cc16135783c),
    (0x3ff2a00000000000, 0xbfc371fc201e9000, 0x3cf178864d27543a),
    (0x3ff2800000000000, 0xbfc29552f81ff000, 0xbd248d301771c408),
    (0x3ff2600000000000, 0xbfc1b72ad52f6000, 0xbd2e80a41811a396),
    (0x3ff2600000000000, 0xbfc1b72ad52f6000, 0xbd2e80a41811a396),
    (0x3ff2400000000000, 0xbfc0d77e7cd09000, 0x3d0a699688e85bf4),
    (0x3ff2400000000000, 0xbfc0d77e7cd09000, 0x3d0a699688e85bf4),
    (0x3ff2200000000000, 0xbfbfec9131dbe000, 0xbd2575545ca333f2),
    (0x3ff2000000000000, 0xbfbe27076e2b0000, 0x3d2a342c2af0003c),
    (0x3ff2000000000000, 0xbfbe27076e2b0000, 0x3d2a342c2af0003c),
    (0x3ff1e00000000000, 0xbfbc5e548f5bc000, 0xbd1d0c57585fbe06),
    (0x3ff1c00000000000, 0xbfba926d3a4ae000, 0x3d253935e85baac8),
    (0x3ff1c00000000000, 0xbfba926d3a4ae000, 0x3d253935e85baac8),
    (0x3ff1a00000000000, 0xbfb8c345d631a000, 0x3d137c294d2f5668),
    (0x3ff1a00000000000, 0xbfb8c345d631a000, 0x3d137c294d2f5668),
    (0x3ff1800000000000, 0xbfb6f0d28ae56000, 0xbd269737c93373da),
    (0x3ff1600000000000, 0xbfb51b073f062000, 0x3d1f025b61c65e57),
    (0x3ff1600000000000, 0xbfb51b073f062000, 0x3d1f025b61c65e57),
    (0x3ff1400000000000, 0xbfb341d7961be000, 0x3d2c5edaccf913df),
    (0x3ff1400000000000, 0xbfb341d7961be000, 0x3d2c5edaccf913df),
    (0x3ff1200000000000, 0xbfb16536eea38000, 0x3d147c5e768fa309),
    (0x3ff1000000000000, 0xbfaf0a30c0118000, 0x3d2d599e83368e91),
    (0x3ff1000000000000, 0xbfaf0a30c0118000, 0x3d2d599e83368e91),
    (0x3ff0e00000000000, 0xbfab42dd71198000, 0x3d1c827ae5d6704c),
    (0x3ff0e00000000000, 0xbfab42dd71198000, 0x3d1c827ae5d6704c),
    (0x3ff0c00000000000, 0xbfa77458f632c000, 0xbd2cfc4634f2a1ee),
    (0x3ff0c00000000000, 0xbfa77458f632c000, 0xbd2cfc4634f2a1ee),
    (0x3ff0a00000000000, 0xbfa39e87b9fec000, 0x3cf502b7f526feaa),
    (0x3ff0a00000000000, 0xbfa39e87b9fec000, 0x3cf502b7f526feaa),
    (0x3ff0800000000000, 0xbf9f829b0e780000, 0xbd2980267c7e09e4),
    (0x3ff0800000000000, 0xbf9f829b0e780000, 0xbd2980267c7e09e4),
    (0x3ff0600000000000, 0xbf97b91b07d58000, 0xbd288d5493faa639),
    (0x3ff0400000000000, 0xbf8fc0a8b0fc0000, 0xbcdf1e7cf6d3a69c),
    (0x3ff0400000000000, 0xbf8fc0a8b0fc0000, 0xbcdf1e7cf6d3a69c),
    (0x3ff0200000000000, 0xbf7fe02a6b100000, 0xbd19e23f0dda40e4),
    (0x3ff0200000000000, 0xbf7fe02a6b100000, 0xbd19e23f0dda40e4),
    (0x3ff0000000000000, 0x0000000000000000, 0x0000000000000000),
    (0x3ff0000000000000, 0x0000000000000000, 0x0000000000000000),
    (0x3fefc00000000000, 0x3f80101575890000, 0xbd10c76b999d2be8),
    (0x3fef800000000000, 0x3f90205658938000, 0xbd23dc5b06e2f7d2),
    (0x3fef400000000000, 0x3f98492528c90000, 0xbd2aa0ba325a0c34),
    (0x3fef000000000000, 0x3fa0415d89e74000, 0x3d0111c05cf1d753),
    (0x3feec00000000000, 0x3fa466aed42e0000, 0xbd2c167375bdfd28),
    (0x3fee800000000000, 0x3fa894aa149fc000, 0xbd197995d05a267d),
    (0x3fee400000000000, 0x3faccb73cdddc000, 0xbd1a68f247d82807),
    (0x3fee200000000000, 0x3faeea31c006c000, 0xbd0e113e4fc93b7b),
    (0x3fede00000000000, 0x3fb1973bd1466000, 0xbd25325d560d9e9b),
    (0x3feda00000000000, 0x3fb3bdf5a7d1e000, 0x3d2cc85ea5db4ed7),
    (0x3fed600000000000, 0x3fb5e95a4d97a000, 0xbd2c69063c5d1d1e),
    (0x3fed400000000000, 0x3fb700d30aeac000, 0x3cec1e8da99ded32),
    (0x3fed000000000000, 0x3fb9335e5d594000, 0x3d23115c3abd47da),
    (0x3fecc00000000000, 0x3fbb6ac88dad6000, 0xbd1390802bf768e5),
    (0x3feca00000000000, 0x3fbc885801bc4000, 0x3d2646d1c65aacd3),
    (0x3fec600000000000, 0x3fbec739830a2000, 0xbd2dc068afe645e0),
    (0x3fec400000000000, 0x3fbfe89139dbe000, 0xbd2534d64fa10afd),
    (0x3fec000000000000, 0x3fc1178e8227e000, 0x3d21ef78ce2d07f2),
    (0x3febe00000000000, 0x3fc1aa2b7e23f000, 0x3d2ca78e44389934),
    (0x3feba00000000000, 0x3fc2d1610c868000, 0x3d039d6ccb81b4a1),
    (0x3feb800000000000, 0x3fc365fcb0159000, 0x3cc62fa8234b7289),
    (0x3feb400000000000, 0x3fc4913d8333b000, 0x3d25837954fdb678),
    (0x3feb200000000000, 0x3fc527e5e4a1b000, 0x3d2633e8e5697dc7),
    (0x3feae00000000000, 0x3fc6574ebe8c1000, 0x3d19cf8b2c3c2e78),
    (0x3feac00000000000, 0x3fc6f0128b757000, 0xbd25118de59c21e1),
    (0x3feaa00000000000, 0x3fc7898d85445000, 0xbd1c661070914305),
    (0x3fea600000000000, 0x3fc8beafeb390000, 0xbd073d54aae92cd1),
    (0x3fea400000000000, 0x3fc95a5adcf70000, 0x3d07f22858a0ff6f),
    (0x3fea000000000000, 0x3fca93ed3c8ae000, 0xbd28724350562169),
    (0x3fe9e00000000000, 0x3fcb31d8575bd000, 0xbd0c358d4eace1aa),
    (0x3fe9c00000000000, 0x3fcbd087383be000, 0xbd2d4bc4595412b6),
    (0x3fe9a00000000000, 0x3fcc6ffbc6f01000, 0xbcf1ec72c5962bd2),
    (0x3fe9600000000000, 0x3fcdb13db0d49000, 0xbd2aff2af715b035),
    (0x3fe9400000000000, 0x3fce530effe71000, 0x3cc212276041f430),
    (0x3fe9200000000000, 0x3fcef5ade4dd0000, 0xbcca211565bb8e11),
    (0x3fe9000000000000, 0x3fcf991c6cb3b000, 0x3d1bcbecca0cdf30),
    (0x3fe8c00000000000, 0x3fd07138604d5800, 0x3cf89cdb16ed4e91),
    (0x3fe8a00000000000, 0x3fd0c42d67616000, 0x3d27188b163ceae9),
    (0x3fe8800000000000, 0x3fd1178e8227e800, 0xbd2c210e63a5f01c),
    (0x3fe8600000000000, 0x3fd16b5ccbacf800, 0x3d2b9acdf7a51681),
    (0x3fe8400000000000, 0x3fd1bf99635a6800, 0x3d2ca6ed5147bdb7),
    (0x3fe8200000000000, 0x3fd214456d0eb800, 0x3d0a87deba46baea),
    (0x3fe7e00000000000, 0x3fd2bef07cdc9000, 0x3d2a9cfa4a5004f4),
    (0x3fe7c00000000000, 0x3fd314f1e1d36000, 0xbd28e27ad3213cb8),
    (0x3fe7a00000000000, 0x3fd36b6776be1000, 0x3d116ecdb0f177c8),
    (0x3fe7800000000000, 0x3fd3c25277333000, 0x3d183b54b606bd5c),
    (0x3fe7600000000000, 0x3fd419b423d5e800, 0x3d08e436ec90e09d),
    (0x3fe7400000000000, 0x3fd4718dc271c800, 0xbd2f27ce0967d675),
    (0x3fe7200000000000, 0x3fd4c9e09e173000, 0xbd2e20891b0ad8a4),
    (0x3fe7000000000000, 0x3fd522ae0738a000, 0x3d2ebe708164c759),
    (0x3fe6e00000000000, 0x3fd57bf753c8d000, 0x3d1fadedee5d40ef),
    (0x3fe6c00000000000, 0x3fd5d5bddf596000, 0xbd0a0b2a08a465dc),
];

const EXP_TABLE_BITS: u32 = 8;
const N_EXP: u64 = 1 << EXP_TABLE_BITS;
const SIGN_BIAS: u64 = 0x800 << EXP_TABLE_BITS;
/// `__exp_data` for N = 256: `invln2N`, the narrow `shift` (`0x1800000000.8p0`), `negln2hiN`,
/// `negln2loN` and the order-5 wide polynomial.
const EXP_INVLN2N: f64 = f64::from_bits(0x40771547652b82fe);
const EXP_SHIFT: f64 = f64::from_bits(0x4238000000008000);
const EXP_NEGLN2HIN: f64 = f64::from_bits(0xbf662e42fefc0000);
const EXP_NEGLN2LON: f64 = f64::from_bits(0x3d2c610ca86c3899);
const EXP_POLY: [f64; 4] = [
    f64::from_bits(0x3fdffffffffffdbd),
    f64::from_bits(0x3fc555555555543c),
    f64::from_bits(0x3fa55555cf16e1ed),
    f64::from_bits(0x3f81111167a4b553),
];
/// `__exp_data.tab` for N = 256: (tail, scale bits) pairs.
#[rustfmt::skip]
const EXP_TAB: [u64; 512] = [
    0x0000000000000000, 0x3ff0000000000000,
    0xbc84e82fc61851ac, 0x3feffb1afa5abcbf,
    0x3c9b3b4f1a88bf6e, 0x3feff63da9fb3335,
    0xbc82985dd8521d32, 0x3feff168143b0281,
    0xbc7160139cd8dc5d, 0x3fefec9a3e778061,
    0x3c651e617061bfbd, 0x3fefe7d42e11bbcc,
    0xbc905e7a108766d1, 0x3fefe315e86e7f85,
    0x3c845fad437fa426, 0x3fefde5f72f654b1,
    0x3c8cd2523567f613, 0x3fefd9b0d3158574,
    0xbc954529642b232f, 0x3fefd50a0e3c1f89,
    0xbc8bce8023f98efa, 0x3fefd06b29ddf6de,
    0x3c8293708ef5c32e, 0x3fefcbd42b72a836,
    0x3c60f74e61e6c861, 0x3fefc74518759bc8,
    0xbc95b9280905b2a4, 0x3fefc2bdf66607e0,
    0x3c90a3e45b33d399, 0x3fefbe3ecac6f383,
    0x3c84f31f32c4b7e7, 0x3fefb9c79b1f3919,
    0x3c979aa65d837b6d, 0x3fefb5586cf9890f,
    0x3c9407fb30d06420, 0x3fefb0f145e46c85,
    0x3c8eb51a92fdeffc, 0x3fefac922b7247f7,
    0xbc9a5d04b3b9911b, 0x3fefa83b23395dec,
    0x3c3ebe3d702f9cd1, 0x3fefa3ec32d3d1a2,
    0xbc937a01f0739546, 0x3fef9fa55fdfa9c5,
    0xbc6a033489906e0b, 0x3fef9b66affed31b,
    0x3c8b8268b04ef0a5, 0x3fef973028d7233e,
    0xbc9556522a2fbd0e, 0x3fef9301d0125b51,
    0xbc9ac46e44a2ebcc, 0x3fef8edbab5e2ab6,
    0xbc5080ef8c4eea55, 0x3fef8abdc06c31cc,
    0xbc65704e90c9f860, 0x3fef86a814f204ab,
    0xbc91c923b9d5f416, 0x3fef829aaea92de0,
    0xbc897cea57e46280, 0x3fef7e95934f312e,
    0x3c80d3e3e95c55af, 0x3fef7a98c8a58e51,
    0x3c56f01429e2b9d2, 0x3fef76a45471c3c2,
    0xbc801b15eaa59348, 0x3fef72b83c7d517b,
    0x3c6e653b2459034b, 0x3fef6ed48695bbc0,
    0xbc8f1ff055de323d, 0x3fef6af9388c8dea,
    0x3c92cc7ea345b7dc, 0x3fef672658375d2f,
    0x3c8b898c3f1353bf, 0x3fef635beb6fcb75,
    0x3c957bfb2876ea9e, 0x3fef5f99f8138a1c,
    0xbc96d99c7611eb26, 0x3fef5be084045cd4,
    0x3c8cdc1873af2155, 0x3fef582f95281c6b,
    0x3c9aecf73e3a2f60, 0x3fef54873168b9aa,
    0xbc9493684653a131, 0x3fef50e75eb44027,
    0xbc8fe782cb86389d, 0x3fef4d5022fcd91d,
    0xbc98e2899077520a, 0x3fef49c18438ce4d,
    0x3c8a6f4144a6c38d, 0x3fef463b88628cd6,
    0x3c9120fcd4f59273, 0x3fef42be3578a819,
    0x3c807a05b0e4047d, 0x3fef3f49917ddc96,
    0x3c89b788c188c9b8, 0x3fef3bdda27912d1,
    0x3c968efde3a8a894, 0x3fef387a6e756238,
    0x3c877afbca90ef84, 0x3fef351ffb82140a,
    0x3c875e18f274487d, 0x3fef31ce4fb2a63f,
    0x3c91512f082876ee, 0x3fef2e85711ece75,
    0x3c80472b981fe7f2, 0x3fef2b4565e27cdd,
    0x3c9a02f0c7d75ec6, 0x3fef280e341ddf29,
    0xbc96b87b3f71085e, 0x3fef24dfe1f56381,
    0xbc803297e78260bf, 0x3fef21ba7591bb70,
    0x3c82f7e16d09ab31, 0x3fef1e9df51fdee1,
    0xbc95b77e5ccd9fbf, 0x3fef1b8a66d10f13,
    0xbc3d219b1a6fbffa, 0x3fef187fd0dad990,
    0xbc91e75c40b4251e, 0x3fef157e39771b2f,
    0x3c8b3782720c0ab4, 0x3fef1285a6e4030b,
    0x3c98a911f1f7785a, 0x3fef0f961f641589,
    0x3c6e149289cecb8f, 0x3fef0cafa93e2f56,
    0xbc61e7c998db7dbb, 0x3fef09d24abd886b,
    0x3c834d754db0abb6, 0x3fef06fe0a31b715,
    0x3c85425c11faadf4, 0x3fef0432edeeb2fd,
    0x3c864201e2ac744c, 0x3fef0170fc4cd831,
    0xbc979517a03e2847, 0x3feefeb83ba8ea32,
    0x3c8fdd395dd3f84a, 0x3feefc08b26416ff,
    0xbc800e2a46da4bee, 0x3feef96266e3fa2d,
    0xbc86a3803b8e5b04, 0x3feef6c55f929ff1,
    0xbc87430803972b34, 0x3feef431a2de883b,
    0xbc924aedcc4b5068, 0x3feef1a7373aa9cb,
    0xbc954de30ae02d94, 0x3feeef26231e754a,
    0xbc9907f81b512d8e, 0x3feeecae6d05d866,
    0xbc94f2487e1c03ec, 0x3feeea401b7140ef,
    0xbc71d1e83e9436d2, 0x3feee7db34e59ff7,
    0x3c914a5432fcb2f4, 0x3feee57fbfec6cf4,
    0xbc991919b3ce1b15, 0x3feee32dc313a8e5,
    0x3c79c3bba5562a2f, 0x3feee0e544ede173,
    0x3c859f48a72a4c6d, 0x3feedea64c123422,
    0xbc85a71612e21658, 0x3feedc70df1c5175,
    0xbc9312607a28698a, 0x3feeda4504ac801c,
    0x3c86421f6f1d24d6, 0x3feed822c367a024,
    0xbc58a78f4817895b, 0x3feed60a21f72e2a,
    0xbc9348a6815fce65, 0x3feed3fb2709468a,
    0xbc7c2c9b67499a1b, 0x3feed1f5d950a897,
    0x3c835c43984d9871, 0x3feecffa3f84b9d4,
    0x3c4363ed60c2ac11, 0x3feece086061892d,
    0xbc632afc8d9473a0, 0x3feecc2042a7d232,
    0x3c9666093b0664ef, 0x3feeca41ed1d0057,
    0xbc95fc5e44de020e, 0x3feec86d668b3237,
    0x3c6ecce1daa10379, 0x3feec6a2b5c13cd0,
    0xbc7ea0148327c42f, 0x3feec4e1e192aed2,
    0x3c93ff8e3f0f1230, 0x3feec32af0d7d3de,
    0xbc7a843ad1a88022, 0x3feec17dea6db7d7,
    0x3c7690cebb7aafb0, 0x3feebfdad5362a27,
    0x3c892ca3bf144e63, 0x3feebe41b817c114,
    0x3c931dbdeb54e077, 0x3feebcb299fddd0d,
    0xbc902c99b04aa8b0, 0x3feebb2d81d8abff,
    0xbc8f94340071a38e, 0x3feeb9b2769d2ca7,
    0x3c73e34f67e67118, 0x3feeb8417f4531ee,
    0xbc87deccdc93a349, 0x3feeb6daa2cf6642,
    0xbc75a3b1197ba0f0, 0x3feeb57de83f4eef,
    0xbc78dec6bd0f385f, 0x3feeb42b569d4f82,
    0x3c81bd2888075068, 0x3feeb2e2f4f6ad27,
    0xbc861246ec7b5cf6, 0x3feeb1a4ca5d920f,
    0xbc896be8ae89ef8f, 0x3feeb070dde910d2,
    0x3c93350518fdd78e, 0x3feeaf4736b527da,
    0xbc88e6ac90348602, 0x3feeae27dbe2c4cf,
    0x3c7b98b72f8a9b05, 0x3feead12d497c7fd,
    0xbc91af7f1365c3ac, 0x3feeac0827ff07cc,
    0x3c9063e1e21c5409, 0x3feeab07dd485429,
    0xbc943a3540d1898a, 0x3feeaa11fba87a03,
    0x3c34c7855019c6ea, 0x3feea9268a5946b7,
    0xbc951f58ddaa8090, 0x3feea84590998b93,
    0x3c9432e62b64c035, 0x3feea76f15ad2148,
    0xbc82e1648e50a17c, 0x3feea6a320dceb71,
    0xbc8ce44a6199769f, 0x3feea5e1b976dc09,
    0x3c95f30eda98a575, 0x3feea52ae6cdf6f4,
    0xbc8c33c53bef4da8, 0x3feea47eb03a5585,
    0x3c917ecda8a72159, 0x3feea3dd1d1929fd,
    0xbc845378892be9ae, 0x3feea34634ccc320,
    0xbc9345f3cee1ae6e, 0x3feea2b9febc8fb7,
    0xbc93cedd78565858, 0x3feea23882552225,
    0xbc85c33fdf910406, 0x3feea1c1c70833f6,
    0x3c5710aa807e1964, 0x3feea155d44ca973,
    0x3c81079ab5789604, 0x3feea0f4b19e9538,
    0xbc93b3efbf5e2228, 0x3feea09e667f3bcd,
    0x3c727df161cd7778, 0x3feea052fa75173e,
    0xbc6a12ad8734b982, 0x3feea012750bdabf,
    0x3c93f9924a05b767, 0x3fee9fdcddd47645,
    0xbc6367efb86da9ee, 0x3fee9fb23c651a2f,
    0xbc87557939a8b5ef, 0x3fee9f9298593ae5,
    0xbc80dc3d54e08851, 0x3fee9f7df9519484,
    0x3c51ed2f56fa9d1a, 0x3fee9f7466f42e87,
    0xbc781f647e5a3ecf, 0x3fee9f75e8ec5f74,
    0xbc88e67a9006c909, 0x3fee9f8286ead08a,
    0xbc86ee4ac08b7db0, 0x3fee9f9a48a58174,
    0x3c86597566977ac8, 0x3fee9fbd35d7cbfd,
    0xbc8619321e55e68a, 0x3fee9feb564267c9,
    0x3c92c0b7028a5c3a, 0x3feea024b1ab6e09,
    0x3c909ccb5e09d4d3, 0x3feea0694fde5d3f,
    0x3c8a30faf49cc78c, 0x3feea0b938ac1cf6,
    0xbc7b32dcb94da51d, 0x3feea11473eb0187,
    0xbc92dad3519d7b5b, 0x3feea17b0976cfdb,
    0x3c94ecfd5467c06b, 0x3feea1ed0130c132,
    0x3c87d51410fd15c2, 0x3feea26a62ff86f0,
    0x3c65ebe1abd66c55, 0x3feea2f336cf4e62,
    0xbc760a3629969871, 0x3feea3878491c491,
    0xbc88a1c52fb3cf42, 0x3feea427543e1a12,
    0x3c8b18c6e3fdef5d, 0x3feea4d2add106d9,
    0xbc9369b6f13b3734, 0x3feea589994cce13,
    0x3c90ec1ddcb1390a, 0x3feea64c1eb941f7,
    0xbc805e843a19ff1e, 0x3feea71a4623c7ad,
    0xbc522cea4f3afa1e, 0x3feea7f4179f5b21,
    0xbc94d450d872576e, 0x3feea8d99b4492ed,
    0x3c7c88549b958471, 0x3feea9cad931a436,
    0x3c90ad675b0e8a00, 0x3feeaac7d98a6699,
    0x3c931143962f7877, 0x3feeabd0a478580f,
    0x3c8db72fc1f0eab4, 0x3feeace5422aa0db,
    0x3c93e9e96f112479, 0x3feeae05bad61778,
    0xbc65b6609cc5e7ff, 0x3feeaf3216b5448c,
    0xbc8dac42a4a38df0, 0x3feeb06a5e0866d9,
    0x3c7bf68359f35f44, 0x3feeb1ae99157736,
    0x3c8b99dd98b1ed84, 0x3feeb2fed0282c8a,
    0xbc93091fa71e3d83, 0x3feeb45b0b91ffc6,
    0xbc7885ad50cbb750, 0x3feeb5c353aa2fe2,
    0xbc5da9b88b6c1e29, 0x3feeb737b0cdc5e5,
    0xbc82d5e85f3e0301, 0x3feeb8b82b5f98e5,
    0xbc6c23f97c90b959, 0x3feeba44cbc8520f,
    0xbc51669428996971, 0x3feebbdd9a7670b3,
    0xbc92434322f4f9aa, 0x3feebd829fde4e50,
    0x3c71f2b2c1c4c014, 0x3feebf33e47a22a2,
    0xbc85ca6cd7668e4b, 0x3feec0f170ca07ba,
    0xbc9294f304f166b6, 0x3feec2bb4d53fe0d,
    0x3c71affc2b91ce27, 0x3feec49182a3f090,
    0xbc8a1e58414c07d3, 0x3feec674194bb8d5,
    0x3c6dd235e10a73bb, 0x3feec86319e32323,
    0xbc79740b58a20091, 0x3feeca5e8d07f29e,
    0xbc87c50422622263, 0x3feecc667b5de565,
    0x3c9165830a2b96c2, 0x3feece7aed8eb8bb,
    0x3c8b1c86e3e231d5, 0x3feed09bec4a2d33,
    0xbc903d5cbe27874b, 0x3feed2c980460ad8,
    0xbc91bbd1d3bcbb15, 0x3feed503b23e255d,
    0x3c5986178980fce0, 0x3feed74a8af46052,
    0x3c90cc319cee31d2, 0x3feed99e1330b358,
    0xbc89472975b1f2a5, 0x3feedbfe53c12e59,
    0x3c8469846e735ab3, 0x3feede6b5579fdbf,
    0x3c7d8157a34b7e7f, 0x3feee0e521356eba,
    0xbc82dfcd978e9db4, 0x3feee36bbfd3f37a,
    0x3c8c8a4e231ebb7d, 0x3feee5ff3a3c2774,
    0x3c8c1a7792cb3387, 0x3feee89f995ad3ad,
    0xbc888c8d11a142e5, 0x3feeeb4ce622f2ff,
    0xbc907b8f4ad1d9fa, 0x3feeee07298db666,
    0x3c889c2ea41433c7, 0x3feef0ce6c9a8952,
    0xbc55c3d956dcaeba, 0x3feef3a2b84f15fb,
    0xbc7274aedac8ff80, 0x3feef68415b749b1,
    0xbc90a40e3da6f640, 0x3feef9728de5593a,
    0x3c85c620ce76df06, 0x3feefc6e29f1c52a,
    0xbc68d6f438ad9334, 0x3feeff76f2fb5e47,
    0xbc8fda52e1b51e41, 0x3fef028cf22749e4,
    0xbc91eee26b588a35, 0x3fef05b030a1064a,
    0xbc32141a7b3e2cd8, 0x3fef08e0b79a6f1f,
    0x3c74ffd70a5fddcd, 0x3fef0c1e904bc1d2,
    0xbc302899507554e5, 0x3fef0f69c3f3a207,
    0xbc91bdfbfa9298ac, 0x3fef12c25bd71e09,
    0xbc80dda2d4c0010c, 0x3fef16286141b33d,
    0x3c736eae30af0cb3, 0x3fef199bdd85529c,
    0xbc8a007daadf8d68, 0x3fef1d1cd9fa652c,
    0x3c8ee3325c9ffd94, 0x3fef20ab5fffd07a,
    0x3c836909391181d3, 0x3fef244778fafb22,
    0x3c84e08fd10959ac, 0x3fef27f12e57d14b,
    0xbc811cd7dbdf9547, 0x3fef2ba88988c933,
    0x3c63cdaf384e1a67, 0x3fef2f6d9406e7b5,
    0xbc7ac28b7bef6621, 0x3fef33405751c4db,
    0x3c676b2c6c921968, 0x3fef3720dcef9069,
    0xbc7030587207b9e1, 0x3fef3b0f2e6d1675,
    0xbc808a1883ccb5d2, 0x3fef3f0b555dc3fa,
    0xbc8cc734592af7fc, 0x3fef43155b5bab74,
    0xbc8fad5d3ffffa6f, 0x3fef472d4a07897c,
    0x3c87752a44f587e8, 0x3fef4b532b08c968,
    0xbc900dae3875a949, 0x3fef4f87080d89f2,
    0x3c85b66fefeef52e, 0x3fef53c8eacaa1d6,
    0x3c74a385a63d07a7, 0x3fef5818dcfba487,
    0x3c5159d9d908a96e, 0x3fef5c76e862e6d3,
    0xbc82919e2040220f, 0x3fef60e316c98398,
    0x3c8c254d16117a68, 0x3fef655d71ff6075,
    0x3c8e5a50d5c192ac, 0x3fef69e603db3285,
    0xbc8d8c329fbd0e03, 0x3fef6e7cd63a8315,
    0x3c843a59ac016b4b, 0x3fef7321f301b460,
    0xbc8ea6e6fbd5f2a6, 0x3fef77d5641c0658,
    0xbc82d52107b43e1f, 0x3fef7c97337b9b5f,
    0xbc63e8e3eab2cbb4, 0x3fef81676b197d17,
    0xbc892ab93b470dc9, 0x3fef864614f5a129,
    0xbc8b7966cd0d2cd9, 0x3fef8b333b16ee12,
    0x3c74b604603a88d3, 0x3fef902ee78b3ff6,
    0xbc776caa4c2ff1cf, 0x3fef953924676d76,
    0x3c83c5ec519d7271, 0x3fef9a51fbc74c83,
    0xbc81d5fc525d9940, 0x3fef9f7977cdb740,
    0xbc8ff7128fd391f0, 0x3fefa4afa2a490da,
    0x3c855cd8aaea3d21, 0x3fefa9f4867cca6e,
    0xbc8dae98e223747d, 0x3fefaf482d8e67f1,
    0x3c8269947c2bed4a, 0x3fefb4aaa2188510,
    0x3c8ec3bc41aa2008, 0x3fefba1bee615a27,
    0xbc83b6137e9afe9e, 0x3fefbf9c1cb6412a,
    0x3c842b94c3a9eb32, 0x3fefc52b376bba97,
    0xbc69fa74878ba7c7, 0x3fefcac948dd7274,
    0x3c8a64a931d185ee, 0x3fefd0765b6e4540,
    0x3c901f3a75ee0efe, 0x3fefd632798844f8,
    0xbc8e37bae43be3ed, 0x3fefdbfdad9cbe14,
    0xbc516a9ce6ed84fa, 0x3fefe1d802243c89,
    0x3c77893b4d91cd9d, 0x3fefe7c1819e90d8,
    0xbc699c7db2effc76, 0x3fefedba3692d514,
    0x3c5305c14160cc89, 0x3feff3c22b8f71f1,
    0x3c64b458677f9840, 0x3feff9d96b2a23d9,
];

fn top12(x: f64) -> u32 {
    (x.to_bits() >> 52) as u32
}

/// `log(x)` as `y + tail`, `ix` the bits of a positive normal `x` (or a normalized subnormal).
fn log_inline(ix: u64) -> (f64, f64) {
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> (52 - POW_LOG_TABLE_BITS)) % N_LOG) as usize;
    let k = (tmp as i64) >> 52;
    let iz = ix.wrapping_sub(tmp & (0xfffu64 << 52));
    let z = f64::from_bits(iz);
    let kd = k as f64;
    let (invc, logc, logctail) = (
        f64::from_bits(T[i].0),
        f64::from_bits(T[i].1),
        f64::from_bits(T[i].2),
    );
    let r = z.mul_add(invc, -1.0);
    let t1 = kd * LN2HI + logc;
    let t2 = t1 + r;
    let lo1 = kd * LN2LO + logctail;
    let lo2 = t1 - t2 + r;
    let ar = A[0] * r;
    let ar2 = r * ar;
    let ar3 = r * ar2;
    let hi = t2 + ar2;
    let lo3 = ar.mul_add(r, -ar2);
    let lo4 = t2 - hi + ar2;
    let p = ar3 * (A[1] + r * A[2] + ar2 * (A[3] + r * A[4] + ar2 * (A[5] + r * A[6])));
    let lo = lo1 + lo2 + lo3 + lo4 + p;
    let y = hi + lo;
    (y, hi - y + lo)
}

/// The results that may overflow or underflow: `scale * (1 + tmp)` without an intermediate
/// rounding, the scale's exponent adjusted by the sign of `ki`.
fn specialcase(tmp: f64, sbits: u64, ki: u64) -> f64 {
    if ki & 0x8000_0000 == 0 {
        // k > 0: the exponent of scale might have overflowed by <= 460.
        let scale = f64::from_bits(sbits.wrapping_sub(1009u64 << 52));
        let y = scale + scale * tmp;
        return y * f64::from_bits(0x7f00_0000_0000_0000); // 0x1p1009
    }
    // k < 0: care in the subnormal range.
    let sbits = sbits.wrapping_add(1022u64 << 52);
    let scale = f64::from_bits(sbits);
    let mut y = scale + scale * tmp;
    if y.abs() < 1.0 {
        let one = if y < 0.0 { -1.0 } else { 1.0 };
        let lo = scale - y + scale * tmp;
        let hi = one + y;
        let lo = one - hi + y + lo;
        y = (hi + lo) - one;
        if y == 0.0 {
            y = f64::from_bits(sbits & 0x8000_0000_0000_0000);
        }
    }
    f64::from_bits(0x0010_0000_0000_0000) * y // 0x1p-1022
}

/// `sign * exp(x + xtail)`.
fn exp_inline(x: f64, xtail: f64, sign_bias: u64) -> f64 {
    let mut abstop = top12(x) & 0x7ff;
    let tiny = top12(f64::from_bits(0x3c90_0000_0000_0000)); // 0x1p-54
    if abstop.wrapping_sub(tiny) >= top12(512.0).wrapping_sub(tiny) {
        if abstop.wrapping_sub(tiny) >= 0x8000_0000 {
            let one = 1.0 + x;
            return if sign_bias != 0 { -one } else { one };
        }
        if abstop >= top12(1024.0) {
            let sign = if sign_bias != 0 { -1.0 } else { 1.0 };
            let (big, small) = (f64::from_bits(0x7000_0000_0000_0000), f64::from_bits(0x1000_0000_0000_0000));
            return if x.to_bits() >> 63 != 0 { sign * small * small } else { sign * big * big };
        }
        abstop = 0;
    }
    // EXP_USE_TOINT_NARROW: z - kd is in [-0.5-2^-16, 0.5].
    let z = EXP_INVLN2N * x;
    let ki = (z + EXP_SHIFT).to_bits() >> 16;
    let kd = ki as i32 as f64;
    let mut r = x + kd * EXP_NEGLN2HIN + kd * EXP_NEGLN2LON;
    r += xtail;
    let idx = (2 * (ki % N_EXP)) as usize;
    let top = ki.wrapping_add(sign_bias) << (52 - EXP_TABLE_BITS);
    let tail = f64::from_bits(EXP_TAB[idx]);
    let sbits = EXP_TAB[idx + 1].wrapping_add(top);
    let r2 = r * r;
    let tmp = tail
        + r
        + r2 * (EXP_POLY[0] + r * EXP_POLY[1])
        + r2 * r2 * (EXP_POLY[2] + r * EXP_POLY[3]);
    if abstop == 0 {
        return specialcase(tmp, sbits, ki);
    }
    let scale = f64::from_bits(sbits);
    scale + scale * tmp
}

/// 0 when `y` is not an integer, 1 when it is odd, 2 when even.
fn checkint(iy: u64) -> u32 {
    let e = (iy >> 52 & 0x7ff) as i32;
    if e < 0x3ff {
        return 0;
    }
    if e > 0x3ff + 52 {
        return 2;
    }
    if iy & ((1u64 << (0x3ff + 52 - e)) - 1) != 0 {
        return 0;
    }
    if iy & (1u64 << (0x3ff + 52 - e)) != 0 {
        return 1;
    }
    2
}

/// Zero, infinity or NaN.
fn zeroinfnan(i: u64) -> bool {
    2u64.wrapping_mul(i).wrapping_sub(1) >= (2 * f64::INFINITY.to_bits()).wrapping_sub(1)
}

/// `x^y` as the runtime's `pow` gives it.
pub(crate) fn crt_pow(x: f64, y: f64) -> f64 {
    let one_bits = 1.0f64.to_bits();
    let inf2 = 2 * f64::INFINITY.to_bits();
    let mut sign_bias = 0u64;
    let mut ix = x.to_bits();
    let iy = y.to_bits();
    let mut topx = top12(x);
    let topy = top12(y);
    if topx.wrapping_sub(0x001) >= 0x7ff - 0x001
        || (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be
    {
        if zeroinfnan(iy) {
            if 2u64.wrapping_mul(iy) == 0 {
                return 1.0;
            }
            if ix == one_bits {
                return 1.0;
            }
            if 2u64.wrapping_mul(ix) > inf2 || 2u64.wrapping_mul(iy) > inf2 {
                return x + y;
            }
            if 2u64.wrapping_mul(ix) == 2 * one_bits {
                return 1.0;
            }
            if (2u64.wrapping_mul(ix) < 2 * one_bits) == (iy >> 63 == 0) {
                return 0.0; // |x| < 1 && y == inf, or |x| > 1 && y == -inf
            }
            return y * y;
        }
        if zeroinfnan(ix) {
            let mut x2 = x * x;
            if ix >> 63 != 0 && checkint(iy) == 1 {
                x2 = -x2;
            }
            return if iy >> 63 != 0 { 1.0 / x2 } else { x2 };
        }
        // x and y are non-zero finite.
        if ix >> 63 != 0 {
            let yint = checkint(iy);
            if yint == 0 {
                return f64::NAN;
            }
            if yint == 1 {
                sign_bias = SIGN_BIAS;
            }
            ix &= 0x7fff_ffff_ffff_ffff;
            topx &= 0x7ff;
        }
        if (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be {
            // sign_bias is 0 here: y is not odd.
            if ix == one_bits {
                return 1.0;
            }
            if (topy & 0x7ff) < 0x3be {
                // |y| < 2^-65: x^y is 1 + y*log(x), rounded.
                return if ix > one_bits { 1.0 + y } else { 1.0 - y };
            }
            let (big, small) = (f64::from_bits(0x7000_0000_0000_0000), f64::from_bits(0x1000_0000_0000_0000));
            return if (ix > one_bits) == (topy < 0x800) { big * big } else { small * small };
        }
        if topx == 0 {
            // A subnormal x, normalized so its exponent becomes negative.
            ix = (f64::from_bits(ix) * f64::from_bits(0x4330_0000_0000_0000)).to_bits(); // 0x1p52
            ix &= 0x7fff_ffff_ffff_ffff;
            ix = ix.wrapping_sub(52u64 << 52);
        }
    }
    let (hi, lo) = log_inline(ix);
    let ehi = y * hi;
    let elo = y * lo + y.mul_add(hi, -ehi);
    exp_inline(ehi, elo, sign_bias)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// A binary64 with a random significand and a leading exponent in `lo .. lo + span`.
        fn positive(&mut self, lo: i64, span: u64) -> f64 {
            let e = (1023 + lo + (self.next() % span) as i64) as u64;
            f64::from_bits((self.next() & 0x000f_ffff_ffff_ffff) | (e << 52))
        }
        fn sign(&mut self) -> f64 {
            if self.next() & 1 == 0 { 1.0 } else { -1.0 }
        }
    }

    /// dc-sbo: the port against the Windows runtime's own `pow` (what `f64::powf` calls under
    /// MSVC) over random arguments in every region of the algorithm: ordinary bases and
    /// exponents, the whole binary64 range, results near overflow and in the subnormal range,
    /// subnormal bases, negative bases with integer exponents, tiny exponents, and SQRTPI's
    /// `(n*pi)^0.5` (`SOFT87_N` cases per region, 1,000,000 by default in a release build).
    #[cfg(target_env = "msvc")]
    /// Native-only: the reference is the MSVC runtime's `pow`, which only Windows has.
    #[test]
    fn crt_pow_equals_the_windows_runtime() {
        let n: usize = std::env::var("SOFT87_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(if cfg!(debug_assertions) { 100_000 } else { 1_000_000 });
        let regions: [(&str, &dyn Fn(&mut Rng) -> (f64, f64)); 8] = [
            ("ordinary", &|r| (r.positive(-10, 20), r.positive(-6, 10) * r.sign())),
            ("whole range", &|r| (r.positive(-1022, 2046), r.positive(-30, 40) * r.sign())),
            ("near overflow and underflow", &|r| {
                let x = r.positive(-4, 8);
                let target = 700.0 + 50.0 * (r.next() % 1000) as f64 / 1000.0;
                let y = target / x.ln() * r.sign();
                (x, y)
            }),
            ("subnormal bases", &|r| (f64::from_bits(r.next() & 0x000f_ffff_ffff_ffff), r.positive(-8, 8) * r.sign())),
            ("negative bases, integer exponents", &|r| {
                (-r.positive(-4, 8), ((r.next() % 400) as f64 - 200.0))
            }),
            ("tiny exponents", &|r| (r.positive(-20, 40), r.positive(-80, 30) * r.sign())),
            ("square roots", &|r| (r.positive(-1022, 2046), 0.5)),
            ("SQRTPI products", &|r| {
                let n = r.positive(-60, 1080);
                (n * std::f64::consts::PI, 0.5)
            }),
        ];
        let mut total = 0;
        for (name, region) in regions {
            let mut rng = Rng(0x5eed_0000 + total as u64 + name.len() as u64);
            let mut differ = 0;
            for _ in 0..n {
                let (x, y) = region(&mut rng);
                if !x.is_finite() {
                    continue;
                }
                // black_box keeps the compiler from replacing a constant-exponent powf (0.5 by sqrt).
                let (x, y) = (std::hint::black_box(x), std::hint::black_box(y));
                let (port, runtime) = (crt_pow(x, y), x.powf(y));
                if port.to_bits() != runtime.to_bits() && !(port.is_nan() && runtime.is_nan()) {
                    if differ < 3 {
                        eprintln!("{name}: pow({x:e}, {y:e}): port {:016x} runtime {:016x}", port.to_bits(), runtime.to_bits());
                    }
                    differ += 1;
                }
            }
            eprintln!("{name}: {differ} of {n} differ");
            total += differ;
        }
        assert_eq!(total, 0);
    }

    /// The special values `pow` defines, on every target.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn special_values() {
        assert_eq!(crt_pow(2.0, 10.0), 1024.0);
        assert_eq!(crt_pow(-2.0, 3.0), -8.0);
        assert!(crt_pow(-2.0, 0.5).is_nan());
        assert_eq!(crt_pow(0.0, -1.0), f64::INFINITY);
        assert_eq!(crt_pow(-0.0, -1.0), f64::NEG_INFINITY);
        assert_eq!(crt_pow(f64::NAN, 0.0), 1.0);
        assert_eq!(crt_pow(1.0, f64::NAN), 1.0);
        assert_eq!(crt_pow(0.5, f64::INFINITY), 0.0);
        assert_eq!(crt_pow(10.0, 400.0), f64::INFINITY);
        assert_eq!(crt_pow(10.0, -400.0), 0.0);
        assert_eq!(crt_pow(4.0, 0.5), 2.0);
    }
}
