//! Exhaustive default-value round-trip across every generated message type.
//!
//! This guards the code generator: if any type produces serialize/deserialize
//! code that doesn't agree (e.g. a mishandled array or nested message), the
//! round-trip for its default value will fail here.

use unitree_msgs::DdsType;

/// Assert `T::default()` survives a full CDR payload round-trip.
fn check<T: DdsType + PartialEq + std::fmt::Debug>() {
    let v = T::default();
    let payload = v.to_cdr();
    let back = T::from_cdr(&payload).expect("decode default");
    assert_eq!(back, v, "{} round-trip", T::TYPE_NAME);
}

macro_rules! check_all {
    ($($path:path),+ $(,)?) => {
        $( check::<$path>(); )+
    };
}

#[test]
fn unitree_api_all() {
    use unitree_msgs::unitree_api::*;
    check_all!(
        Request,
        RequestHeader,
        RequestIdentity,
        RequestLease,
        RequestPolicy,
        Response,
        ResponseHeader,
        ResponseStatus,
    );
}

#[test]
fn unitree_go_all() {
    use unitree_msgs::unitree_go::*;
    check_all!(
        AudioData,
        BmsCmd,
        BmsState,
        Error,
        Go2FrontVideoData,
        HeightMap,
        IMUState,
        InterfaceConfig,
        LidarState,
        LowCmd,
        LowState,
        MotorCmd,
        MotorCmds,
        MotorState,
        MotorStates,
        PathPoint,
        Req,
        Res,
        SportModeCmd,
        SportModeState,
        TimeSpec,
        UwbState,
        UwbSwitch,
        WirelessController,
    );
}

#[test]
fn unitree_hg_all() {
    use unitree_msgs::unitree_hg::*;
    check_all!(
        BmsCmd,
        BmsState,
        HandCmd,
        HandState,
        IMUState,
        LowCmd,
        LowState,
        MainBoardState,
        MotorCmd,
        MotorState,
        PressSensorState,
    );
}
