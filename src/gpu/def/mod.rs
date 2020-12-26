pub mod compute;
pub mod graphics;
pub mod render_passes;

pub mod desc_set_layouts {
    use {
        super::*,
        crate::gpu::driver::descriptor_set_layout_binding,
        gfx_hal::pso::{DescriptorSetLayoutBinding, ShaderStageFlags},
    };

    pub const BLEND: [DescriptorSetLayoutBinding; 2] = [
        descriptor_set_layout_binding(
            0, // blend
            ShaderStageFlags::FRAGMENT,
            READ_ONLY_IMG,
        ),
        descriptor_set_layout_binding(
            1, // base
            ShaderStageFlags::FRAGMENT,
            READ_ONLY_IMG,
        ),
    ];
    pub const CALC_VERTEX_ATTRS: [DescriptorSetLayoutBinding; 4] = [
        descriptor_set_layout_binding(
            0, // idx_buf
            ShaderStageFlags::COMPUTE,
            READ_ONLY_BUF,
        ),
        descriptor_set_layout_binding(
            1, // src_buf
            ShaderStageFlags::COMPUTE,
            READ_ONLY_BUF,
        ),
        descriptor_set_layout_binding(
            2, // dst_buf
            ShaderStageFlags::COMPUTE,
            READ_WRITE_BUF,
        ),
        descriptor_set_layout_binding(
            3, // write_mask
            ShaderStageFlags::COMPUTE,
            READ_ONLY_BUF,
        ),
    ];
    pub const DECODE_RGB_RGBA: [DescriptorSetLayoutBinding; 2] = [
        descriptor_set_layout_binding(
            0, // pixel_buf
            ShaderStageFlags::COMPUTE,
            READ_ONLY_BUF,
        ),
        descriptor_set_layout_binding(
            1, // image
            ShaderStageFlags::COMPUTE,
            READ_WRITE_IMG,
        ),
    ];
    pub const DRAW_MESH: [DescriptorSetLayoutBinding; 3] = [
        descriptor_set_layout_binding(
            0, // color
            ShaderStageFlags::FRAGMENT,
            READ_ONLY_IMG,
        ),
        descriptor_set_layout_binding(
            1, // metal_rough
            ShaderStageFlags::FRAGMENT,
            READ_ONLY_IMG,
        ),
        descriptor_set_layout_binding(
            2, // normal
            ShaderStageFlags::FRAGMENT,
            READ_ONLY_IMG,
        ),
    ];
    pub const SINGLE_READ_ONLY_IMG: [DescriptorSetLayoutBinding; 1] =
        [descriptor_set_layout_binding(
            0,
            ShaderStageFlags::FRAGMENT,
            READ_ONLY_IMG,
        )];
    pub const SKYDOME: [DescriptorSetLayoutBinding; 1] = [descriptor_set_layout_binding(
        0, // ?
        ShaderStageFlags::FRAGMENT,
        READ_WRITE_IMG,
    )];
}

pub mod push_consts {
    use {gfx_hal::pso::ShaderStageFlags, std::ops::Range};

    pub type ShaderRange = (ShaderStageFlags, Range<u32>);

    pub const BLEND: [ShaderRange; 2] = [
        (ShaderStageFlags::VERTEX, 0..64),
        (ShaderStageFlags::FRAGMENT, 64..72),
    ];
    pub const CALC_VERTEX_ATTRS: [ShaderRange; 1] = [(ShaderStageFlags::COMPUTE, 0..8)];
    pub const DECODE_RGB_RGBA: [ShaderRange; 1] = [(ShaderStageFlags::COMPUTE, 0..4)];
    pub const DRAW_POINT_LIGHT: [ShaderRange; 2] = [
        (ShaderStageFlags::VERTEX, 0..64),
        (ShaderStageFlags::FRAGMENT, 0..0),
    ];
    pub const DRAW_RECT_LIGHT: [ShaderRange; 2] = [
        (ShaderStageFlags::VERTEX, 0..64),
        (ShaderStageFlags::FRAGMENT, 0..0),
    ];
    pub const DRAW_SPOTLIGHT: [ShaderRange; 2] = [
        (ShaderStageFlags::VERTEX, 0..64),
        (ShaderStageFlags::FRAGMENT, 0..0),
    ];
    pub const DRAW_SUNLIGHT: [ShaderRange; 2] = [
        (ShaderStageFlags::VERTEX, 0..64),
        (ShaderStageFlags::FRAGMENT, 0..0),
    ];
    pub const FONT: [ShaderRange; 2] = [
        (ShaderStageFlags::VERTEX, 0..64),
        (ShaderStageFlags::FRAGMENT, 64..80),
    ];
    pub const FONT_OUTLINE: [ShaderRange; 2] = [
        (ShaderStageFlags::VERTEX, 0..64),
        (ShaderStageFlags::FRAGMENT, 64..96),
    ];
    pub const SKYDOME: [ShaderRange; 0] = [];
    pub const TEXTURE: [ShaderRange; 1] = [(ShaderStageFlags::VERTEX, 0..80)];
    pub const VERTEX_MAT4: [ShaderRange; 1] = [(ShaderStageFlags::VERTEX, 0..64)];
}

pub use self::{compute::Compute, graphics::Graphics};

use {
    super::{BlendMode, MaskMode, MatteMode},
    crate::pak::IndexType,
    gfx_hal::{
        format::Format,
        pso::{BufferDescriptorFormat, BufferDescriptorType, DescriptorType, ImageDescriptorType},
    },
};

const READ_ONLY_BUF: DescriptorType = DescriptorType::Buffer {
    format: BufferDescriptorFormat::Structured {
        dynamic_offset: false,
    },
    ty: BufferDescriptorType::Storage { read_only: true },
};
const READ_ONLY_IMG: DescriptorType = DescriptorType::Image {
    ty: ImageDescriptorType::Sampled { with_sampler: true },
};
const READ_WRITE_BUF: DescriptorType = DescriptorType::Buffer {
    format: BufferDescriptorFormat::Structured {
        dynamic_offset: false,
    },
    ty: BufferDescriptorType::Storage { read_only: false },
};
const READ_WRITE_IMG: DescriptorType = DescriptorType::Image {
    ty: ImageDescriptorType::Storage { read_only: false },
};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct CalcVertexAttrsComputeMode {
    pub idx_ty: IndexType,
    pub skin: bool,
}

impl CalcVertexAttrsComputeMode {
    pub const U16: Self = Self {
        idx_ty: IndexType::U16,
        skin: false,
    };
    pub const U16_SKIN: Self = Self {
        idx_ty: IndexType::U16,
        skin: true,
    };
    pub const U32: Self = Self {
        idx_ty: IndexType::U32,
        skin: false,
    };
    pub const U32_SKIN: Self = Self {
        idx_ty: IndexType::U32,
        skin: true,
    };
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ColorRenderPassMode {
    pub fmt: Format,
    pub preserve: bool,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum ComputeMode {
    CalcVertexAttrs(CalcVertexAttrsComputeMode),
    DecodeRgbRgba,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct DrawRenderPassMode {
    pub depth: Format,
    pub geom_buf: Format,
    pub light: Format,
    pub output: Format,
    pub pre_fx: bool,
    pub post_fx: bool,
}

// TODO: Break this up a bit more?
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum GraphicsMode {
    Blend(BlendMode),
    Font,
    FontOutline,
    Gradient,
    GradientTransparency,
    DrawLine,
    DrawMesh,
    DrawPointLight,
    DrawRectLight,
    DrawSpotlight,
    DrawSunlight,
    Mask(MaskMode),
    Matte(MatteMode),
    Skydome,
    Texture,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum RenderPassMode {
    Color(ColorRenderPassMode),
    Draw(DrawRenderPassMode),
}
