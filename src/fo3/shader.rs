//! Fallout 3 / New Vegas shader property semantics used by the native scene
//! extractor and the material extras written to GLB.

/// BSLightingShaderProperty shader type values used by Fallout 3.
pub const SHADER_TYPE_DEFAULT: u32 = 0;
pub const SHADER_TYPE_ENVIRONMENT_MAP: u32 = 1;
pub const SHADER_TYPE_GLOW_SHADER: u32 = 2;
pub const SHADER_TYPE_PARALLAX: u32 = 3;
pub const SHADER_TYPE_FACE_TINT: u32 = 4;
pub const SHADER_TYPE_SKIN_TINT: u32 = 5;
pub const SHADER_TYPE_HAIR_TINT: u32 = 6;

/// BSLightingShaderProperty shader flags 1.
pub const SHADER_FLAG1_SPECULAR: u32 = 1 << 0;
pub const SHADER_FLAG1_VERTEX_ALPHA: u32 = 1 << 3;
pub const SHADER_FLAG1_ENVIRONMENT_MAPPING: u32 = 1 << 7;
pub const SHADER_FLAG1_PARALLAX: u32 = 1 << 11;
pub const SHADER_FLAG1_MODEL_SPACE_NORMALS: u32 = 1 << 12;
pub const SHADER_FLAG1_REFRACTION: u32 = 1 << 15;

/// BSLightingShaderProperty shader flags 2.
pub const SHADER_FLAG2_DOUBLE_SIDED: u32 = 1 << 4;
pub const SHADER_FLAG2_VERTEX_COLORS: u32 = 1 << 5;
pub const SHADER_FLAG2_GLOW_MAP: u32 = 1 << 6;
pub const SHADER_FLAG2_MULTI_LAYER_PARALLAX: u32 = 1 << 24;
pub const SHADER_FLAG2_SOFT_LIGHTING: u32 = 1 << 25;
pub const SHADER_FLAG2_BACK_LIGHTING: u32 = 1 << 27;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FalloutShaderFeatures {
    pub glow_map: bool,
    pub specular: bool,
    pub parallax: bool,
    pub environment_mapping: bool,
    pub double_sided: bool,
    pub vertex_colors: bool,
    pub vertex_alpha: bool,
    pub soft_lighting: bool,
    pub back_lighting: bool,
    pub translucent_candidate: bool,
}

impl FalloutShaderFeatures {
    pub fn from_flags(shader_type: u32, shader_flags_1: u32, shader_flags_2: u32) -> Self {
        let glow_map =
            shader_flags_2 & SHADER_FLAG2_GLOW_MAP != 0 || shader_type == SHADER_TYPE_GLOW_SHADER;
        let specular = shader_flags_1 & SHADER_FLAG1_SPECULAR != 0
            || shader_flags_1 & SHADER_FLAG1_MODEL_SPACE_NORMALS != 0;
        let parallax = shader_flags_1 & SHADER_FLAG1_PARALLAX != 0
            || shader_flags_2 & SHADER_FLAG2_MULTI_LAYER_PARALLAX != 0;
        let environment_mapping = shader_flags_1 & SHADER_FLAG1_ENVIRONMENT_MAPPING != 0
            || shader_type == SHADER_TYPE_ENVIRONMENT_MAP;
        let soft_lighting = shader_flags_2 & SHADER_FLAG2_SOFT_LIGHTING != 0;
        let back_lighting = shader_flags_2 & SHADER_FLAG2_BACK_LIGHTING != 0;
        let translucent_candidate = soft_lighting
            || back_lighting
            || matches!(shader_type, SHADER_TYPE_SKIN_TINT | SHADER_TYPE_HAIR_TINT);
        Self {
            glow_map,
            specular,
            parallax,
            environment_mapping,
            double_sided: shader_flags_2 & SHADER_FLAG2_DOUBLE_SIDED != 0,
            vertex_colors: shader_flags_2 & SHADER_FLAG2_VERTEX_COLORS != 0,
            vertex_alpha: shader_flags_1 & SHADER_FLAG1_VERTEX_ALPHA != 0,
            soft_lighting,
            back_lighting,
            translucent_candidate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glow_and_specular_are_flag_driven() {
        let features = FalloutShaderFeatures::from_flags(
            SHADER_TYPE_DEFAULT,
            SHADER_FLAG1_SPECULAR,
            SHADER_FLAG2_GLOW_MAP,
        );
        assert!(features.glow_map);
        assert!(features.specular);
        assert!(!features.environment_mapping);
    }

    #[test]
    fn shader_type_preserves_glow_and_translucency_semantics() {
        let features = FalloutShaderFeatures::from_flags(
            SHADER_TYPE_GLOW_SHADER,
            0,
            SHADER_FLAG2_BACK_LIGHTING,
        );
        assert!(features.glow_map);
        assert!(features.translucent_candidate);
    }

    #[test]
    fn unrelated_slot_flags_do_not_enable_glow() {
        let features = FalloutShaderFeatures::from_flags(
            SHADER_TYPE_DEFAULT,
            SHADER_FLAG1_SPECULAR,
            SHADER_FLAG2_VERTEX_COLORS,
        );
        assert!(!features.glow_map);
        assert!(!features.translucent_candidate);
    }
}
