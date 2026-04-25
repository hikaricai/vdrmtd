use three_d::*;

fn create_lines(
    context: &Context,
    paths: &[Vec<(f32, f32, f32)>],
    thickness: f32,
    color: Srgba,
) -> Gm<InstancedMesh, ColorMaterial> {
    let mut transformations = Vec::new();
    for path in paths {
        for window in path.windows(2) {
            let p0 = vec3(window[0].0, window[0].1, window[0].2);
            let p1 = vec3(window[1].0, window[1].1, window[1].2);
            let dir = p1 - p0;
            let length = dir.magnitude();
            if length > 0.0001 {
                let dir_n = dir / length;
                // 计算从 X 轴正向到 dir_n 的旋转
                let rot = Quat::from_arc(vec3(1.0, 0.0, 0.0), dir_n, Some(vec3(0.0, 1.0, 0.0)));
                let transform = Mat4::from_translation(p0)
                    * Mat4::from(rot)
                    * Mat4::from_nonuniform_scale(length, thickness, thickness);
                transformations.push(transform);
            }
        }
    }

    let instances = Instances {
        transformations,
        ..Default::default()
    };

    let mesh = InstancedMesh::new(context, &instances, &CpuMesh::cylinder(16));
    let material = ColorMaterial::new_opaque(
        context,
        &CpuMaterial {
            albedo: color,
            ..Default::default()
        },
    );
    Gm::new(mesh, material)
}

fn virtual_boards() -> Vec<Vec<(f32, f32, f32)>> {
    let screen: vdrm_alg::Screen = vdrm_alg::screens()[1];
    let angle_s = 90f32 - 22.5f32;
    let angle_e = 90f32 + 22.5f32;
    let screen_s = vdrm_alg::mirror_points_f(angle_s.to_radians(), &screen.points);
    let screen_e = vdrm_alg::mirror_points_f(angle_e.to_radians(), &screen.points);

    let num_points = 10;
    let mut boards = vec![Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    for i in 0..num_points {
        let angle = angle_s + (angle_e - angle_s) * (i as f32) / ((num_points - 1) as f32);
        let points = vdrm_alg::mirror_points_f(angle.to_radians(), &screen.points);
        let points: [(f32, f32, f32); 4] = points.try_into().unwrap();
        for (board, p) in boards.iter_mut().zip(points) {
            board.push(p);
        }
    }
    boards.extend([screen_s, screen_e]);

    let screen_s = vdrm_alg::mirror_points_f(90f32.to_radians(), &screen.points);
    let p_s = Vector3::from(screen_s[0]);
    let p_e = vec3(0., 1., 1.);
    let p_dir = p_e - p_s;
    println!("p_s: {:?}", p_s);
    println!("p_e: {:?}", p_e);
    println!("p_dir: {:?}", p_dir);

    // 将 boards 里的点全部按照 p_dir 移动
    for board in &mut boards {
        for p in board {
            p.0 += p_dir.x;
            p.1 += p_dir.y;
            p.2 += p_dir.z;
        }
    }

    boards
}

fn main() {
    let window = Window::new(WindowSettings {
        title: "RGBH Capture Tool".to_string(),
        max_size: Some((192, 192)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    // 1. 初始化相机 (使用正交相机，消除透视形变)
    let camera_distance = 2.5;
    let mut camera = Camera::new_orthographic(
        window.viewport(),
        vec3(0.0, 0.0, camera_distance),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        3.5, // height: 视口高度足够包裹 2.0 大小的模型
        0.1,
        10.0, // far: 恢复正常的远裁剪面
    );
    let mut control = OrbitControl::new(*camera.target(), 1.5, 5.0);

    let camera_cl = camera.clone();

    // 2. 加载外部模型 (Taxi.glb)
    // .glb 文件自带所有材质和贴图，不需要像 .obj 一样加载三个文件
    let mut cpu_model: CpuModel =
        three_d_asset::io::load_and_deserialize("asserts/Taxi.glb").unwrap();

    // 如果模型很大或很小，我们计算它的尺寸并将其缩放到我们固定的 2x2x2 盒子内 (也就是最大半径为 1)
    let mut model_aabb = AxisAlignedBoundingBox::EMPTY;
    for primitive in cpu_model.geometries.iter_mut() {
        model_aabb.expand_with_aabb(&primitive.geometry.compute_aabb());
    }

    let size = model_aabb.size();
    let max_size = size.x.max(size.y).max(size.z);
    let scale = 2.0 / max_size; // 让模型最长的一边刚好是 2.0

    // 将模型平移到中心并应用缩放
    let center = model_aabb.center();
    // 强制给模型计算法线 (PhysicalMaterial 渲染必须要有法线数据)
    for primitive in cpu_model.geometries.iter_mut() {
        primitive.geometry.compute_normals();
    }

    // 由于 three-d-asset 加载的模型内部可能有各种层级变换，直接改几何体的矩阵有时不生效
    // 我们选择先生成 Model，然后再统一给 Model 赋予变换
    let mut model = Model::<PhysicalMaterial>::new(&context, &cpu_model).unwrap();

    let base_transform = Mat4::from_scale(scale) * Mat4::from_translation(-center);

    println!("Original Model Size: {:?}", size);
    println!("Max Size: {}", max_size);

    // 重新添加光源 (PhysicalMaterial 需要光照)
    let ambient = AmbientLight::new(&context, 0.5, Srgba::WHITE);
    let directional = DirectionalLight::new(&context, 2.0, Srgba::WHITE, &vec3(-1.0, -1.0, -1.0));

    // 添加一个平面正方形，默认 CpuMesh::square() 边长为 2.0 (halfsize=1.0)
    // 缩放 0.5 使得边长为 1.0
    // let mut sq_mesh = CpuMesh::square();
    // sq_mesh.transform(&Mat4::from_scale(0.5)).unwrap();
    // let sq_material = ColorMaterial::new_transparent(
    //     &context,
    //     &CpuMaterial {
    //         albedo: Srgba::new(255, 50, 50, 128), // 半透明红色 (alpha: 128)
    //         ..Default::default()
    //     },
    // );
    // let mut square = Gm::new(Mesh::new(&context, &sq_mesh), sq_material);

    // 创建坐标轴
    let axes = Axes::new(&context, 0.05, 2.0);

    // 获取并创建虚拟板的线条模型
    let board_paths = virtual_boards();
    let mut boards_model = create_lines(
        &context,
        &board_paths,
        0.01,                         // 线条粗细
        Srgba::new_opaque(0, 255, 0), // 绿色线条
    );
    boards_model.set_transformation(Mat4::from_angle_z(degrees(180.)));

    // 用于记录模型当前的累积旋转和按键状态
    // 初始化时直接给 rotation 赋这个基础的居中缩放变换，让后面的键盘增量旋转都在这个基础上进行
    let mut rotation = base_transform;
    let mut keys: std::collections::HashSet<Key> = std::collections::HashSet::new();
    let mut z_mov = 0.0f32;
    // 3. 渲染循环
    window.render_loop(move |mut frame_input| {
        let viewport = frame_input.viewport;
        camera.set_viewport(viewport);
        control.handle_events(&mut camera, &mut frame_input.events);

        let mut space_pressed = false;
        // --- 收集按键状态 ---
        for event in frame_input.events.iter() {
            match event {
                Event::KeyPress { kind, .. } => {
                    if *kind == Key::Space {
                        space_pressed = true;
                    }
                    if *kind == Key::Enter {
                        camera = camera_cl.clone();
                        control = OrbitControl::new(*camera.target(), 1.5, 5.0);
                        break;
                    }
                    keys.insert(*kind);
                }
                Event::KeyRelease { kind, .. } => {
                    keys.remove(kind);
                }
                _ => {}
            }
        }

        // --- 根据按键状态计算当前帧的“局部旋转增量” ---
        let speed = 100.0 * frame_input.elapsed_time as f32 / 1000.0; // 约每秒 100 度的旋转速度

        let mut d_pitch = 0.0_f32;
        let mut d_yaw = 0.0_f32;
        let mut d_roll = 0.0_f32;

        if keys.contains(&Key::W) {
            d_pitch += speed;
        } // 机头向上 (绕局部 X 轴)
        if keys.contains(&Key::S) {
            d_pitch -= speed;
        } // 机头向下
        if keys.contains(&Key::A) {
            d_yaw -= speed;
        } // 机头向左 (绕局部 Y 轴)
        if keys.contains(&Key::D) {
            d_yaw += speed;
        } // 机头向右
        if keys.contains(&Key::Q) {
            d_roll += speed;
        } // 左倾翻滚 (绕局部 Z 轴)
        if keys.contains(&Key::E) {
            d_roll -= speed;
        } // 右倾翻滚
        if keys.contains(&Key::I) {
            z_mov += speed / 100.;
        }
        if keys.contains(&Key::K) {
            z_mov -= speed / 100.;
        }

        // 构造当前帧的局部旋转矩阵
        let delta_rot = Mat4::from_angle_y(degrees(d_yaw))
            * Mat4::from_angle_x(degrees(d_pitch))
            * Mat4::from_angle_z(degrees(d_roll));

        // 将局部旋转累加到模型的总旋转中
        // (右乘 delta_rot 表示基于当前的局部坐标轴继续旋转，而不是基于世界的固定坐标轴)
        rotation = rotation * delta_rot;
        for part in model.iter_mut() {
            part.set_transformation(Mat4::from_translation(Vector3::new(0., 0., z_mov)) * rotation);
        }
        // 让正方形跟随模型一起旋转，但稍微向 Z 轴正向偏移一点，以免被完全埋在中间
        // square.set_transformation(rotation * Mat4::from_translation(vec3(0.0, 0.0, 1.2)));
        // 让虚拟板的线条跟随旋转
        // boards_model.set_transformation(rotation);

        let objects: Box<dyn Iterator<Item = _>> = if !space_pressed {
            Box::new(model.into_iter().chain(&boards_model))
        } else {
            Box::new(model.into_iter())
        };
        // 渲染到屏幕
        let screen = frame_input.screen();
        screen
            .clear(ClearState::color_and_depth(0.0, 0.0, 0.0, 1.0, 1.0))
            // 现在模型使用了 PhysicalMaterial，需要传入光源进行渲染
            .render(&camera, objects, &[&ambient, &directional]);

        if space_pressed {
            // --- 0.17.0 正确的 API 路径 ---
            // 在 0.17 中，必须指定泛型类型或者使用具体的 read_color 方法
            let pixels = screen.read_color::<[u8; 4]>();
            let depth_values = screen.read_depth();

            // 计算线性深度 (H)
            let near = camera.z_near();
            let far = camera.z_far();

            // --- 移除通过 AABB 顶点转换求深度的逻辑 ---
            // 因为如果将世界空间中的 AABB 转到相机空间，随着相机旋转，AABB 的投影在深度方向会拉长（对角线最长可达 2*sqrt(3)）
            // 这会导致 min_z 和 max_z 发生改变，从而使得归一化深度随视角变化，边缘不再是 128！

            // --- 采用固定相机空间深度的逻辑 ---
            // 因为相机目前到 target (0,0,0) 的距离是 camera_distance (2.5)
            // 模型缩放后的最大边长是 2.0，所以它的最大半径为 1.0
            let distance_to_target = camera_distance;
            let model_radius = 1.0;

            // 我们固定的体积显示器捕获深度范围：
            let min_z = distance_to_target - model_radius;
            let max_z = distance_to_target + model_radius;

            let mut min_z_raw = f32::MAX;
            let mut max_z_raw = f32::MIN;
            // 利用模型自身的 min_z 和 max_z 将深度归一化到 0-255
            let mut h_data = Vec::with_capacity(depth_values.len());
            for &z_raw in depth_values.iter() {
                if z_raw >= 0.9999 {
                    h_data.push(0);
                } else {
                    // 记录原始深度范围
                    if z_raw < min_z_raw {
                        min_z_raw = z_raw;
                    }
                    if z_raw > max_z_raw {
                        max_z_raw = z_raw;
                    }
                    // 对于正交相机，深度 z_raw (0.0 到 1.0) 与真实的线性深度_raw深度_raw (0.0 到 1.0) 与真实的线性深度是正向的直接线性关系
                    // z_raw = 0.0 对应 near 面，z_raw = 1.0 对应 far 面
                    let z_linear = near + z_raw * (far - near);

                    // 动态归一化：球体最靠近相机的点映射为 255 (白)，最远点映射为 0 (黑)
                    let normalized_depth = (z_linear - min_z) / (max_z - min_z);

                    // 为了保证球的边缘 (即深度刚好在 min_z 和 max_z 的正中间时) 值为 128
                    // 127.5 需要四舍五入
                    let h_val = (255.0 * (1.0 - normalized_depth)).round().clamp(0.0, 255.0) as u8;

                    h_data.push(h_val);
                }
            }

            // 保存 RGBH 渲染图
            save_rgbh(viewport.width, viewport.height, &pixels, &h_data);
            // 添加采样保存深度的调用 (这里步长选 16，保证文本宽度适中)
            save_depth_txt(viewport.width, viewport.height, &h_data, 16);
            println!(">>> RGBH 图片和 txt 深度采样已生成！");
            println!("(min_z, max_z): {:?}", (min_z, max_z));
            println!("(near, far): {:?}", (near, far));
            println!("(min_z_raw, max_z_raw): {:?}", (min_z_raw, max_z_raw));
        }

        FrameOutput::default()
    });
}

// 将深度图采样输出为 txt 文件
fn save_depth_txt(w: u32, h: u32, h_raw: &[u8], step: usize) {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("output_depth.txt").unwrap();
    // 按照指定的步长 step 遍历高和宽
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            // OpenGL 屏幕坐标系原点在左下角，这里转成左上角原点，与图像像素保持一致
            let src_idx = ((h - 1 - y) * w + x) as usize;
            let d = h_raw[src_idx];
            // 格式化为固定 3 位的数字
            write!(file, "{:3} ", d).unwrap();
        }
        writeln!(file).unwrap();
    }
}

fn save_rgbh(w: u32, h: u32, rgb_raw: &[[u8; 4]], h_raw: &[u8]) {
    use image::{Rgb, RgbImage};
    let mut canvas = RgbImage::new(w * 2, h);
    for y in 0..h {
        for x in 0..w {
            // three-d 0.17 版本的 read_color() 内部已经自动做了 flip_y，所以颜色数组的(0,0)是左上角
            let rgb_idx = (y * w + x) as usize;
            // 但是 read_depth() 并没有做 flip_y，它的(0,0)仍然在屏幕的左下角，需要手动翻转 Y
            let depth_idx = ((h - 1 - y) * w + x) as usize;

            let r = rgb_raw[rgb_idx][0];
            let g = rgb_raw[rgb_idx][1];
            let b = rgb_raw[rgb_idx][2];
            let depth = h_raw[depth_idx];

            canvas.put_pixel(x, y, Rgb([r, g, b]));
            canvas.put_pixel(x + w, y, Rgb([depth, depth, depth]));
        }
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let path = format!("output_rgbh_{}.png", ts.as_secs());
    canvas.save(path).unwrap();
}
