use {
    bytemuck::cast_slice,
    inline_spirv::inline_spirv,
    screen_13::prelude::*,
    std::{mem::size_of, sync::Arc},
};

// Min/max sampler reduction is commonly used to create depth buffer mip-maps for use with gpu-based
// visibility determination.
//
// Support for min/max sampling is core to Vulkan 1.2 however different graphics cards may have
// varying supported properties which are detailed by the physical device property structures.
//
// Note that this example only reduces the sample "depth image" once, and it does not fully occupy
// the compute units of the GPU by using larger local group sizes.
fn main() -> Result<(), DriverError> {
    pretty_env_logger::init();

    let mut render_graph = RenderGraph::new();
    let device = Arc::new(Device::create_headless(DeviceInfo::new())?);
    let size = 4;

    // The 4x4 depth image will have pixels that look like this:
    //   0.0   1.0   2.0   3.0
    //   4.0   5.0   6.0   7.0
    //   8.0   9.0  10.0  11.0
    //  12.0  13.0  14.0  15.0
    let depth_image = fill_depth_image(&device, &mut render_graph, size)?;

    // The 2x2 reduced image has undefined data until we wait on the results later
    let reduced_image = reduce_depth_image(&device, &mut render_graph, depth_image)?;

    // Create a result buffer so we can read back the results
    let result_buf = wait_for_results(&device, render_graph, reduced_image)?;

    // The result data will look like this - we have reduced each 4x4 pixel group into the maximum
    // value of each group:
    //   5.0   7.0
    //  13.0  15.0
    let result_data: &[f32] = cast_slice(Buffer::mapped_slice(&result_buf));

    println!("{result_data:?}");

    assert_eq!(result_data.len(), 4);

    assert_eq!(result_data[0], 5.0);
    assert_eq!(result_data[1], 7.0);
    assert_eq!(result_data[2], 13.0);
    assert_eq!(result_data[3], 15.0);

    Ok(())
}

fn fill_depth_image(
    device: &Arc<Device>,
    render_graph: &mut RenderGraph,
    size: u32,
) -> Result<ImageNode, DriverError> {
    let info = ImageInfo::new_2d(
        vk::Format::D32_SFLOAT,
        size,
        size,
        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
    )
    .build();
    let ImageInfo {
        fmt,
        ty,
        tiling,
        usage,
        flags,
        ..
    } = info;

    // Sometimes required because support is not 100% common: Check min/max reduction support
    // https://vulkan.gpuinfo.org/listdevicescoverage.php?extension=VK_EXT_sampler_filter_minmax&platform=all
    let fmt_props = Device::format_properties(device, fmt);
    if !fmt_props.optimal_tiling_features.contains(
        vk::FormatFeatureFlags::SAMPLED_IMAGE
            | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
            | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_MINMAX,
    ) {
        // In this case you might just fall back to a compute shader algorthm
        warn!("Requested image does not support min/max reduction");

        return Err(DriverError::Unsupported);
    }

    // If this is not supported you would need a fallback algorithm (this duplicates the check
    // we already performed above, it's just a different way to go about finding the answer)
    assert!(
        device
            .physical_device
            .sampler_filter_minmax_properties
            .single_component_formats
    );

    // Not required, but good practice: Check image format support
    let image_fmt_props =
        Device::image_format_properties(device, fmt, ty.into(), tiling, usage, flags)?
            .ok_or(DriverError::Unsupported)?;
    if size > image_fmt_props.max_extent.width || size > image_fmt_props.max_extent.height {
        // In this case you might use a smaller image
        warn!("Requested image is too big");

        return Err(DriverError::Unsupported);
    }

    // You could check this if you needed to reduce multiple channel images:
    // device.physical_device.sampler_filter_minmax_properties.image_component_mapping

    let depth_data = render_graph.bind_node(Buffer::create_from_slice(
        device,
        vk::BufferUsageFlags::TRANSFER_SRC,
        cast_slice(&(0..size.pow(2)).map(|x| x as f32).collect::<Box<_>>()),
    )?);
    let depth_image = render_graph.bind_node(Image::create(device, info)?);
    render_graph.copy_buffer_to_image(depth_data, depth_image);

    Ok(depth_image)
}

fn reduce_depth_image(
    device: &Arc<Device>,
    render_graph: &mut RenderGraph,
    depth_image: ImageNode,
) -> Result<ImageNode, DriverError> {
    let depth_info = render_graph.node_info(depth_image);

    assert_eq!(depth_info.width, depth_info.height);

    // (We use R32_SFLOAT because D32_SFLOAT has very low support for the STORAGE usage and most
    // implementations would be reading the image elsewhere instead of using it as a depth image)
    let reduced_info = ImageInfo::new_2d(
        vk::Format::R32_SFLOAT,
        depth_info.width >> 1,
        depth_info.height >> 1,
        vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
    )
    .build();
    let reduced_image = render_graph.bind_node(Arc::new(Image::create(device, reduced_info)?));

    render_graph
        .begin_pass("Reduce depth image")
        .bind_pipeline(&Arc::new(ComputePipeline::create(
            device,
            ComputePipelineInfo::default(),
            Shader::new_compute(
                inline_spirv!(
                    r#"#version 460 core
                
                    layout(binding = 0) uniform sampler2D depth_image;
                    layout(binding = 1) writeonly uniform image2D reduced_image;

                    void main() {
                        ivec2 reduced_size = imageSize(reduced_image);
                        vec2 sample_xy = vec2(gl_GlobalInvocationID.xy) + 0.5;
                        vec4 sample_val = texture(depth_image, sample_xy / vec2(reduced_size));

                        ivec2 store_xy = ivec2(gl_GlobalInvocationID.xy);
                        imageStore(reduced_image, store_xy, sample_val);
                    }"#,
                    comp
                )
                .as_slice(),
            )
            .image_sampler(
                0,
                SamplerInfo::LINEAR.reduction_mode(vk::SamplerReductionMode::MAX),
            ),
        )?))
        .read_descriptor(0, depth_image)
        .write_descriptor(1, reduced_image)
        .record_compute(move |compute, _| {
            compute.dispatch(reduced_info.width, reduced_info.height, 1);
        });

    Ok(reduced_image)
}

fn wait_for_results(
    device: &Arc<Device>,
    mut render_graph: RenderGraph,
    reduced_image: ImageNode,
) -> Result<Arc<Buffer>, DriverError> {
    let reduced_info = render_graph.node_info(reduced_image);
    let result_len = (reduced_info.width * reduced_info.height) as vk::DeviceSize
        * size_of::<f32>() as vk::DeviceSize;
    let result_buf = render_graph.bind_node(Arc::new(Buffer::create(
        device,
        BufferInfo::new_mappable(result_len, vk::BufferUsageFlags::TRANSFER_DST),
    )?));

    render_graph.copy_image_to_buffer(reduced_image, result_buf);

    let result_buf = render_graph.unbind_node(result_buf);
    render_graph
        .resolve()
        .submit(&mut HashPool::new(device), 0, 0)?
        .wait_until_executed()?;

    Ok(result_buf)
}
