device_driver::create_device!(
    device_name: Ads1299Registers,
    dsl: {
        config {
            type RegisterAddressType = u8;
            type DefaultByteOrder = LE;
        }
        register Id {
            type Access = RO;
            const ADDRESS = 0x00;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0xE0;

            rev_id: uint = 5..8,
            dev_id: uint = 2..4,
            nu_ch: uint = 0..2,
        },
        register Config1 {
            type Access = RW;
            const ADDRESS = 0x01;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x96;

            daisy_en: bool = 6,
            clk_en: bool = 5,
            dr: uint as enum SampleRate {
                KSps16,
                KSps8,
                KSps4,
                KSps2,
                KSps1,
                Sps500,
                Sps250,
            } = 0..3,
        },
        register Config2 {
            type Access = RW;
            const ADDRESS = 0x02;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0xC0;

            int_cal: bool = 4,
            cal_amp: bool = 2,
            cal_freq: uint as enum CalFreq {
                FclkBy21,
                FclkBy20,
                DoNotUse,
                DC,
            } = 0..2,
        },
        register Config3 {
            type Access = RW;
            const ADDRESS = 0x03;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x60;

            pd_refbuf: bool = 7,
            bias_meas: bool = 4,
            biasref_int: bool = 3,
            pd_bias: bool = 2,
            bias_loff_sens: bool = 1,
            bias_stat: bool = 0,
        },
        register Loff {
            type Access = RW;
            const ADDRESS = 0x04;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            comp_th: uint as enum CompThreshPos {
                _95,
                _92_5,
                _90,
                _87_5,
                _85,
                _80,
                _75,
                _70,
            } = 5..8,
            ilead_off: uint as enum ILeadOff {
                _6nA,
                _24nA,
                _6uA,
                _24uA,
            } = 2..4,
            flead_off: uint as enum FLeadOff {
                Dc,
                Ac7_8,
                Ac31_2,
                AcFdrBy4,
            } = 0..2,
        },
        register ChSet {
            type Access = RW;
            const ADDRESS = 0x05;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x61;
            const REPEAT = {
                count: 8,
                stride: 1,
            };

            pd: bool = 7,
            gain: uint as enum Gain {
                X1,
                X2,
                X4,
                X6,
                X8,
                X12,
                X24,
            } = 4..7,
            srb2: bool = 3,
            mux: uint as enum Mux {
                NormalElectrodeInput,
                InputShorted,
                RldMeasure,
                MVDD,
                TemperatureSensor,
                TestSignal,
                RldDrp,
                RldDrn,
            } = 0..3,
        },
        register BiasSensP {
            type Access = RW;
            const ADDRESS = 0x0D;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            biasp8: bool = 7,
            biasp7: bool = 6,
            biasp6: bool = 5,
            biasp5: bool = 4,
            biasp4: bool = 3,
            biasp3: bool = 2,
            biasp2: bool = 1,
            biasp1: bool = 0,
        },
        register BiasSensN {
            type Access = RW;
            const ADDRESS = 0x0E;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            biasn8: bool = 7,
            biasn7: bool = 6,
            biasn6: bool = 5,
            biasn5: bool = 4,
            biasn4: bool = 3,
            biasn3: bool = 2,
            biasn2: bool = 1,
            biasn1: bool = 0,
        },
        register LoffSensP {
            type Access = RW;
            const ADDRESS = 0x0F;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            loffp8: bool = 7,
            loffp7: bool = 6,
            loffp6: bool = 5,
            loffp5: bool = 4,
            loffp4: bool = 3,
            loffp3: bool = 2,
            loffp2: bool = 1,
            loffp1: bool = 0,
        },
        register LoffSensN {
            type Access = RW;
            const ADDRESS = 0x10;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            loffn8: bool = 7,
            loffn7: bool = 6,
            loffn6: bool = 5,
            loffn5: bool = 4,
            loffn4: bool = 3,
            loffn3: bool = 2,
            loffn2: bool = 1,
            loffn1: bool = 0,
        },
        register LoffFlip {
            type Access = RW;
            const ADDRESS = 0x11;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            loff_flip8: bool = 7,
            loff_flip7: bool = 6,
            loff_flip6: bool = 5,
            loff_flip5: bool = 4,
            loff_flip4: bool = 3,
            loff_flip3: bool = 2,
            loff_flip2: bool = 1,
            loff_flip1: bool = 0,
        },
        register LoffStatP {
            type Access = RO;
            const ADDRESS = 0x12;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            in8p_off: bool = 7,
            in7p_off: bool = 6,
            in6p_off: bool = 5,
            in5p_off: bool = 4,
            in4p_off: bool = 3,
            in3p_off: bool = 2,
            in2p_off: bool = 1,
            in1p_off: bool = 0,
        },
        register LoffStatN {
            type Access = RO;
            const ADDRESS = 0x13;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            in8n_off: bool = 7,
            in7n_off: bool = 6,
            in6n_off: bool = 5,
            in5n_off: bool = 4,
            in4n_off: bool = 3,
            in3n_off: bool = 2,
            in2n_off: bool = 1,
            in1n_off: bool = 0,
        },
        register Gpio {
            type Access = RW;
            const ADDRESS = 0x14;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x0F;

            gpiod4: bool = 7,
            gpiod3: bool = 6,
            gpiod2: bool = 5,
            gpiod1: bool = 4,
            gpioc4: bool = 3,
            gpioc3: bool = 2,
            gpioc2: bool = 1,
            gpioc1: bool = 0,
        },
        register Misc1 {
            type Access = RW;
            const ADDRESS = 0x15;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            srb1: bool = 5,
        },
        register Misc2 {
            type Access = RW;
            const ADDRESS = 0x16;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;
        },
        register Config4 {
            type Access = RW;
            const ADDRESS = 0x17;
            const SIZE_BITS = 8;
            const RESET_VALUE = 0x00;

            single_shot: bool = 3,
            pd_loff_comp: bool = 1,
        },
    }
);
