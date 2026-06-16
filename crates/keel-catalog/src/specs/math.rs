//! `math` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "math",
        name: "PI",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "The mathematical constant π (3.14159…).",
    },
    BuiltinMethod {
        namespace: "math",
        name: "E",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "The mathematical constant e (2.71828…).",
    },
    BuiltinMethod {
        namespace: "math",
        name: "sqrt",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the square root of x.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "pow",
        params: &[
            BuiltinParam {
                name: "x",
                ty: TySpec::Float,
                optional: false,
            },
            BuiltinParam {
                name: "y",
                ty: TySpec::Float,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return x raised to the power y.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "exp",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return e raised to the power x.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "log",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the natural logarithm of x.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "log2",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the base-2 logarithm of x.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "log10",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the base-10 logarithm of x.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "sin",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the sine of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "math",
        name: "cos",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the cosine of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "math",
        name: "tan",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the tangent of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "math",
        name: "asin",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arcsine of x in radians.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "acos",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arccosine of x in radians.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "atan",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arctangent of x in radians.",
    },
    BuiltinMethod {
        namespace: "math",
        name: "atan2",
        params: &[
            BuiltinParam {
                name: "y",
                ty: TySpec::Float,
                optional: false,
            },
            BuiltinParam {
                name: "x",
                ty: TySpec::Float,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the four-quadrant arctangent of y and x in radians.",
    },
];
