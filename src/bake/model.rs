use {
    super::{
        Model as ModelAsset, {get_filename_key, get_path},
    },
    crate::{
        math::{quat, vec3, Mat4, Quat, Sphere, Vec3},
        pak::{model::Mesh, IndexType, Model, ModelId, PakBuf},
    },
    gltf::{import, mesh::Mode, Node, Primitive},
    std::{
        collections::{HashMap, HashSet},
        path::Path,
        u16,
    },
};

pub fn bake_model<P1: AsRef<Path>, P2: AsRef<Path>>(
    project_dir: P1,
    asset_filename: P2,
    asset: &ModelAsset,
    pak: &mut PakBuf,
) -> ModelId {
    let key = get_filename_key(&project_dir, &asset_filename);
    if let Some(id) = pak.id(&key) {
        return id.as_model().unwrap();
    }

    info!("Processing asset: {}", key);

    let dir = asset_filename.as_ref().parent().unwrap();
    let src = get_path(&dir, asset.src(), project_dir);

    let mut mesh_names: HashMap<&str, Option<&str>> = HashMap::default();
    if let Some(meshes) = asset.meshes() {
        for mesh in meshes {
            mesh_names
                .entry(mesh.src_name())
                .or_insert_with(|| mesh.dst_name());
        }
    }

    let (doc, bufs, _) = import(src).unwrap();
    let nodes = doc
        .nodes()
        .filter(|node| node.mesh().is_some())
        .map(|node| (node.mesh().unwrap(), node))
        .filter(|(mesh, _)| {
            if mesh_names.is_empty() {
                return true;
            }

            if let Some(name) = mesh.name() {
                return mesh_names.contains_key(name);
            }

            false
        })
        .map(|(mesh, node)| (mesh.name().unwrap_or_default(), mesh, node))
        .collect::<Vec<_>>();
    let mut idx_buf = vec![];
    let mut vertex_buf = vec![];
    let mut meshes = vec![];

    // The whole model will use either 16 or 32 bit indices
    let tiny_idx = nodes.iter().all(|(_, mesh, _)| {
        mesh.primitives()
            .map(|primitive| (tri_mode(&primitive), primitive))
            .filter(|(mode, _)| mode.is_some())
            .map(|(mode, primitive)| (mode.unwrap(), primitive))
            .all(|(_, primitive)| {
                primitive
                    .reader(|buf| bufs.get(buf.index()).map(|data| &*data.0))
                    .read_positions()
                    .expect("Unable to read mesh positions")
                    .count()
                    <= u16::MAX as _
            })
    });
    let idx_ty = if tiny_idx {
        IndexType::U16
    } else {
        IndexType::U32
    };

    let mut idx_write = vec![];

    let mut all_indices = vec![];
    let mut base_idx = 0;
    for (name, mesh, node) in nodes {
        if meshes.len() == u16::MAX as usize {
            warn!("Maximum number of meshes supported per model have been loaded, others have been skipped");
            break;
        }

        let dst_name = mesh_names
            .get(name)
            .map(|name| name.map(|name| name.to_owned()))
            .unwrap_or(None);
        let skin = node.skin();
        let transform = get_transform(&node);
        let mut all_positions = vec![];
        let mut idx_count = 0;
        let mut vertex_count = 0;
        let vertex_offset = vertex_buf.len() as u32;

        for (mode, primitive) in mesh
            .primitives()
            .map(|primitive| (tri_mode(&primitive), primitive))
            .filter(|(mode, _)| mode.is_some())
            .map(|(mode, primitive)| (mode.unwrap(), primitive))
        {
            assert_eq!(mode, TriangleMode::List);

            let data = primitive.reader(|buf| bufs.get(buf.index()).map(|data| &*data.0));
            let indices = data
                .read_indices()
                .expect("Unable to read mesh indices")
                .into_u32()
                .collect::<Vec<_>>();
            let positions = data.read_positions().unwrap().collect::<Vec<_>>();
            let tex_coords = data
                .read_tex_coords(0)
                .expect("Unable to read mesh texture cooordinates")
                .into_f32()
                .collect::<Vec<_>>();

            all_indices.extend_from_slice(&indices);
            all_positions.extend_from_slice(&positions);
            idx_count += indices.len() as u32;
            vertex_count += positions.len();

            // For each index, store a true if it has not yet appeared in the list
            let mut seen = HashSet::new();
            for idx in &indices {
                idx_write.push(!seen.contains(&idx));
                seen.insert(idx);
            }

            match idx_ty {
                IndexType::U16 => indices
                    .iter()
                    .for_each(|idx| idx_buf.extend_from_slice(&(*idx as u16).to_ne_bytes())),
                IndexType::U32 => indices
                    .iter()
                    .for_each(|idx| idx_buf.extend_from_slice(&idx.to_ne_bytes())),
            }

            let (joints, weights) = if skin.is_some() {
                let joints = data.read_joints(0).unwrap().into_u16().collect::<Vec<_>>();
                let weights = data.read_weights(0).unwrap().into_f32().collect::<Vec<_>>();
                (Some(joints), Some(weights))
            } else {
                (None, None)
            };

            for idx in 0..positions.len() {
                vertex_buf.extend_from_slice(&positions[idx][0].to_ne_bytes());
                vertex_buf.extend_from_slice(&positions[idx][1].to_ne_bytes());
                vertex_buf.extend_from_slice(&positions[idx][2].to_ne_bytes());
                vertex_buf.extend_from_slice(&tex_coords[idx][0].to_ne_bytes());
                vertex_buf.extend_from_slice(&tex_coords[idx][1].to_ne_bytes());

                if skin.is_some() {
                    let joints = joints.as_ref().unwrap();
                    let weights = weights.as_ref().unwrap();

                    vertex_buf.extend_from_slice(&joints[idx][0].to_ne_bytes());
                    vertex_buf.extend_from_slice(&joints[idx][1].to_ne_bytes());
                    vertex_buf.extend_from_slice(&joints[idx][2].to_ne_bytes());
                    vertex_buf.extend_from_slice(&joints[idx][3].to_ne_bytes());
                    vertex_buf.extend_from_slice(&weights[idx][0].to_ne_bytes());
                    vertex_buf.extend_from_slice(&weights[idx][1].to_ne_bytes());
                    vertex_buf.extend_from_slice(&weights[idx][2].to_ne_bytes());
                    vertex_buf.extend_from_slice(&weights[idx][3].to_ne_bytes());
                }
            }
        }

        meshes.push(Mesh::new(
            dst_name,
            base_idx..base_idx + idx_count,
            vertex_count as _,
            vertex_offset,
            Sphere::from_point_cloud(
                all_positions
                    .iter()
                    .map(|position| vec3(position[0], position[1], position[2])),
            ),
            transform,
            skin.map(|s| {
                let joints = s.joints().map(|node| node.name().unwrap().to_owned());
                let inv_binds = s
                    .reader(|buf| bufs.get(buf.index()).map(|data| &*data.0))
                    .read_inverse_bind_matrices()
                    .unwrap()
                    .map(|ibp| Mat4::from_cols_array_2d(&ibp));

                joints.zip(inv_binds).into_iter().collect()
            }),
        ));
        base_idx += idx_count;
    }

    // The write mask is a compression structure. It is used to allow the compute shaders which
    // calculate extra vertex attributes (normal and tangent) to run in a lock-free manner. This
    // *could* be done at runtime but it is very small and takes a bit of effort to figure out.
    let mask_len = idx_write.len() + 31 >> 5;

    // The index-write vector has unused space which we must pad out for the next loop
    for _ in idx_write.len()..mask_len << 5 {
        idx_write.push(false);
    }

    // Turn the vec of bools into a vec of u32s where each bit is one of the bools
    let mut write_mask = vec![];
    for idx in 0..mask_len {
        let idx = idx << 5;
        let mask: u32 = 0
            | 1 << idx_write[idx] as u32
            | 2 << idx_write[idx + 1] as u32
            | 3 << idx_write[idx + 2] as u32
            | 4 << idx_write[idx + 3] as u32
            | 5 << idx_write[idx + 4] as u32
            | 6 << idx_write[idx + 5] as u32
            | 7 << idx_write[idx + 6] as u32
            | 8 << idx_write[idx + 7] as u32
            | 9 << idx_write[idx + 8] as u32
            | 10 << idx_write[idx + 9] as u32
            | 11 << idx_write[idx + 10] as u32
            | 12 << idx_write[idx + 11] as u32
            | 13 << idx_write[idx + 12] as u32
            | 14 << idx_write[idx + 13] as u32
            | 15 << idx_write[idx + 14] as u32
            | 16 << idx_write[idx + 15] as u32
            | 17 << idx_write[idx + 16] as u32
            | 18 << idx_write[idx + 17] as u32
            | 19 << idx_write[idx + 18] as u32
            | 20 << idx_write[idx + 19] as u32
            | 21 << idx_write[idx + 20] as u32
            | 22 << idx_write[idx + 21] as u32
            | 23 << idx_write[idx + 22] as u32
            | 24 << idx_write[idx + 23] as u32
            | 25 << idx_write[idx + 24] as u32
            | 26 << idx_write[idx + 25] as u32
            | 27 << idx_write[idx + 26] as u32
            | 28 << idx_write[idx + 27] as u32
            | 29 << idx_write[idx + 28] as u32
            | 30 << idx_write[idx + 29] as u32
            | 31 << idx_write[idx + 30] as u32
            | 32 << idx_write[idx + 31] as u32;
        write_mask.extend_from_slice(&mask.to_ne_bytes());
    }

    // Pak this asset
    pak.push_model(
        key,
        Model::new(meshes, idx_ty, idx_buf, vertex_buf, write_mask),
    )
}

fn create_write_mask<I: ExactSizeIterator<Item = u32>>(indices: I) -> Vec<u8> {
    let idx_len = indices.len();
    let mask_len = idx_len + 31 >> 5;
    let mut write_idx = Vec::with_capacity(mask_len);
    let mut write_seen = HashSet::new();

    // For each index, store a true if it has not yet appeared in the list
    for idx in indices {
        let seen = write_seen.contains(&idx);
        write_idx.push(!seen);
        write_seen.insert(idx);
    }

    // The write index vector has unused space which we must pad out so the below logic is easier to write
    for _ in idx_len..mask_len {
        write_idx.push(false);
    }

    // Turn the vec of bools into a vec of u32s where each bit is one of the bools
    let mut write_mask = vec![];
    for idx in 0..mask_len {
        let idx = idx << 5;
        let mask: u32 = 0
            | 1 << write_idx[idx] as u32
            | 2 << write_idx[idx + 1] as u32
            | 3 << write_idx[idx + 2] as u32
            | 4 << write_idx[idx + 3] as u32
            | 5 << write_idx[idx + 4] as u32
            | 6 << write_idx[idx + 5] as u32
            | 7 << write_idx[idx + 6] as u32
            | 8 << write_idx[idx + 7] as u32
            | 9 << write_idx[idx + 8] as u32
            | 10 << write_idx[idx + 9] as u32
            | 11 << write_idx[idx + 10] as u32
            | 12 << write_idx[idx + 11] as u32
            | 13 << write_idx[idx + 12] as u32
            | 14 << write_idx[idx + 13] as u32
            | 15 << write_idx[idx + 14] as u32
            | 16 << write_idx[idx + 15] as u32
            | 17 << write_idx[idx + 16] as u32
            | 18 << write_idx[idx + 17] as u32
            | 19 << write_idx[idx + 18] as u32
            | 20 << write_idx[idx + 19] as u32
            | 21 << write_idx[idx + 20] as u32
            | 22 << write_idx[idx + 21] as u32
            | 23 << write_idx[idx + 22] as u32
            | 24 << write_idx[idx + 23] as u32
            | 25 << write_idx[idx + 24] as u32
            | 26 << write_idx[idx + 25] as u32
            | 27 << write_idx[idx + 26] as u32
            | 28 << write_idx[idx + 27] as u32
            | 29 << write_idx[idx + 28] as u32
            | 30 << write_idx[idx + 29] as u32
            | 31 << write_idx[idx + 30] as u32
            | 32 << write_idx[idx + 31] as u32;
        write_mask.extend_from_slice(&mask.to_ne_bytes());
    }

    write_mask
}

fn get_transform(node: &Node) -> Option<Mat4> {
    let (translation, rotation, scale) = node.transform().decomposed();
    let rotation = quat(rotation[0], rotation[1], rotation[2], rotation[3]);
    let scale = vec3(scale[0], scale[1], scale[2]);
    let translation = vec3(translation[0], translation[1], translation[2]);
    if scale != Vec3::one()
        || rotation != Quat::identity()
        || translation != Vec3::zero()
    {
        Some(Mat4::from_scale_rotation_translation(
            scale,
            rotation,
            translation,
        ))
    } else {
        None
    }
}

fn node_stride(node: &Node) -> usize {
    if node.skin().is_some() {
        88
    } else {
        64
    }
}

fn tri_mode(primitive: &Primitive) -> Option<TriangleMode> {
    match primitive.mode() {
        Mode::TriangleFan => Some(TriangleMode::Fan),
        Mode::Triangles => Some(TriangleMode::List),
        Mode::TriangleStrip => Some(TriangleMode::Strip),
        _ => None,
    }
}

#[derive(Debug, PartialEq)]
enum TriangleMode {
    Fan,
    List,
    Strip,
}
