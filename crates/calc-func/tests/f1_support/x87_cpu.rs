//! Exact test expectations for measured vendor-dependent hardware-x87 rows.
//! Excel itself on Intel has not been measured for these inputs. These expectations describe
//! calc-func's hardware instructions. Excel build 20527 on the AMD machine independently
//! agrees with all 15 AMD results (2026-10-31, CalcFunc-Evidence b047ad6a).
//! That observation does not measure Excel on Intel or replace other Excel answers.

#![allow(dead_code)] // Shared by a unit test and an integration test, with different callers.

pub struct Measurement {
    pub vendor: &'static str,
    pub bits: u64,
    pub measured_on: &'static str,
}

pub struct Row {
    pub function: &'static str,
    pub input: u64,
    pub software_bits: u64,
    pub hardware: [Measurement; 2],
}

const AMD_LN: &str = "AMD Ryzen 7 4800H, Windows; existing x87_excel_ground_truth.tsv pin";
const AMD_EXP: &str = "AMD Ryzen 7 4800H, Windows; existing x87_excel_ground_truth.tsv pin and four clean-room EXP differences";
const AMD_COS: &str =
    "AMD Ryzen 7 4800H, Windows; existing COS.json cos-tan-held-0043 pin (Excel build 20228)";
const INTEL: &str = "dna-koderbot, Linux x86_64, Intel Haswell under KVM, 2026-10-30; codex-runs/dnacalc-setup/native-tests.log and codex-runs/phase3-land/elementary-hard.jsonl";

// (function, input, AMD result, Intel result, software result, AMD measurement source).
// The original assertion stopped on the first difference. Measuring all 249 EXP/LN rows found
// 13 vendor-dependent LN inputs and one EXP input; COS has one among 31,200 pinned witness rows.
pub const ROWS: &[Row] = &[
    Row::new(
        "EXP",
        0xc00aff210cf81b26,
        0x3fa186f1e0bf9266,
        0x3fa186f1e0bf9265,
        0x3fa186f1e0bf9266,
        AMD_EXP,
    ),
    Row::new(
        "LN",
        0x11f544676cf1bf2f,
        0xc07fddf2ad7e124d,
        0xc07fddf2ad7e124c,
        0xc07fddf2ad7e124c,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x1eb393c13e56f40a,
        0xc07708d724444078,
        0xc07708d724444077,
        0xc07708d724444077,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x2d20badbb5556728,
        0xc06a12f75473f581,
        0xc06a12f75473f580,
        0xc06a12f75473f580,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x2d3693905c941f14,
        0xc069f331dcfe7f08,
        0xc069f331dcfe7f07,
        0xc069f331dcfe7f07,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x2e68258fb8991289,
        0xc0684b9c09fc3ba6,
        0xc0684b9c09fc3ba5,
        0xc0684b9c09fc3ba5,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x420be30a078fed72,
        0x40376deebfb7f46e,
        0x40376deebfb7f46d,
        0x40376deebfb7f46d,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x4574658f6970c8b6,
        0x404e9eafb5cc42c6,
        0x404e9eafb5cc42c5,
        0x404e9eafb5cc42c5,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x461d6906690fdadb,
        0x40512261215c415e,
        0x40512261215c415d,
        0x40512261215c415d,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x4d7d2f67e7585ef8,
        0x4062ca44831b0c3a,
        0x4062ca44831b0c39,
        0x4062ca44831b0c39,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x4fb4742155e8cc52,
        0x4065dd65ac726cee,
        0x4065dd65ac726ced,
        0x4065dd65ac726ced,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x501baf60a3904e63,
        0x40666c2b3ad75615,
        0x40666c2b3ad75614,
        0x40666c2b3ad75614,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x559d2eada7837182,
        0x406e0dc1bc5d2a56,
        0x406e0dc1bc5d2a55,
        0x406e0dc1bc5d2a55,
        AMD_LN,
    ),
    Row::new(
        "LN",
        0x6fffb3b90594a629,
        0x4080a82aa75e37f7,
        0x4080a82aa75e37f6,
        0x4080a82aa75e37f6,
        AMD_LN,
    ),
    Row::new(
        "COS",
        0x4062a6de023b2c9c,
        0xbf86a175abef3a94,
        0xbf86a175abef3a93,
        0xbf86a175abef3a94,
        AMD_COS,
    ),
];

pub fn find(function: &str, input: u64) -> Option<&'static Row> {
    ROWS.iter()
        .find(|row| row.function == function && row.input == input)
}

impl Row {
    const fn new(
        function: &'static str,
        input: u64,
        amd: u64,
        intel: u64,
        software_bits: u64,
        amd_source: &'static str,
    ) -> Self {
        Self {
            function,
            input,
            software_bits,
            hardware: [
                Measurement {
                    vendor: "AuthenticAMD",
                    bits: amd,
                    measured_on: amd_source,
                },
                Measurement {
                    vendor: "GenuineIntel",
                    bits: intel,
                    measured_on: INTEL,
                },
            ],
        }
    }

    pub fn for_vendor(&self, vendor: &str) -> &Measurement {
        self.hardware
            .iter()
            .find(|m| m.vendor == vendor)
            .unwrap_or_else(|| {
                panic!(
                    "{}({:#018x}): no measured hardware-x87 result for CPUID vendor {vendor:?}",
                    self.function, self.input
                )
            })
    }

    #[cfg(target_arch = "x86_64")]
    pub fn hardware_bits(&self) -> u64 {
        // CPUID leaf 0 is available on every x86_64 CPU. The vendor string is EBX,
        // EDX, ECX (not the hypervisor vendor from leaf 0x40000000).
        let id = core::arch::x86_64::__cpuid(0);
        let mut vendor = [0; 12];
        vendor[..4].copy_from_slice(&id.ebx.to_le_bytes());
        vendor[4..8].copy_from_slice(&id.edx.to_le_bytes());
        vendor[8..].copy_from_slice(&id.ecx.to_le_bytes());
        self.for_vendor(core::str::from_utf8(&vendor).expect("CPUID vendor is not ASCII"))
            .bits
    }

    pub fn expected_bits(&self) -> u64 {
        #[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
        {
            self.hardware_bits()
        }
        #[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
        {
            self.software_bits
        }
    }
}
