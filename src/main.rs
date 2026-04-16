use three_d::*;

fn main() {
    let window = Window::new(WindowSettings {
        title: "RGBH Capture Tool".to_string(),
        max_size: Some((720, 720)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    // 1. 初始化相机 (使用正交相机，消除透视形变)
    let mut camera = Camera::new_orthographic(
        window.viewport(),
        vec3(0.0, 0.0, 2.5),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        2.5, // height: 因为球体直径是 2.0，所以视野高度设置为 2.5 就能完整包裹球体
        0.1,
        10.0,
    );
    let mut control = OrbitControl::new(*camera.target(), 1.5, 5.0);

    // 2. 创建模型 (使用 Gm 和 Mesh)
    let mesh = Mesh::new(&context, &CpuMesh::sphere(32));
    // 使用 ColorMaterial (无光照材质)，让模型每个地方都一样亮
    let material = ColorMaterial::new_opaque(
        &context,
        &CpuMaterial {
            albedo: Srgba::new_opaque(0, 150, 255),
            ..Default::default()
        },
    );
    let model = Gm::new(mesh, material);

    // 打印模型的包围盒(AABB)范围
    let aabb = model.aabb();
    println!("Model AABB Min: {:?}", aabb.min());
    println!("Model AABB Max: {:?}", aabb.max());
    println!("Model Size: {:?}", aabb.size());

    // 创建坐标轴
    let axes = Axes::new(&context, 0.05, 2.0);

    // 3. 渲染循环
    window.render_loop(move |mut frame_input| {
        let viewport = frame_input.viewport;
        camera.set_viewport(viewport);
        control.handle_events(&mut camera, &mut frame_input.events);

        // 渲染到屏幕
        let screen = frame_input.screen();
        screen
            .clear(ClearState::color_and_depth(0.1, 0.1, 0.1, 1.0, 1.0))
            // 传入空的光源数组 &[]，因为 ColorMaterial 不需要光照
            .render(&camera, model.into_iter().chain(&axes), &[]);

        for event in frame_input.events.iter() {
            if let Event::KeyPress {
                kind: Key::Space, ..
            } = event
            {
                // --- 0.17.0 正确的 API 路径 ---
                // 在 0.17 中，必须指定泛型类型或者使用具体的 read_color 方法
                let pixels = screen.read_color::<[u8; 4]>();
                let depth_values = screen.read_depth();

                // 计算线性深度 (H)
                let near = camera.z_near();
                let far = camera.z_far();
                
                // --- 计算模型在当前视角下的准确深度范围 ---
                let view_matrix = camera.view();
                let min_pos = aabb.min();
                let max_pos = aabb.max();
                // 提取 AABB 的 8 个顶点
                let corners = [
                    vec3(min_pos.x, min_pos.y, min_pos.z),
                    vec3(min_pos.x, min_pos.y, max_pos.z),
                    vec3(min_pos.x, max_pos.y, min_pos.z),
                    vec3(min_pos.x, max_pos.y, max_pos.z),
                    vec3(max_pos.x, min_pos.y, min_pos.z),
                    vec3(max_pos.x, min_pos.y, max_pos.z),
                    vec3(max_pos.x, max_pos.y, min_pos.z),
                    vec3(max_pos.x, max_pos.y, max_pos.z),
                ];

                let mut min_z = f32::MAX;
                let mut max_z = f32::MIN;

                for corner in &corners {
                    // 转换到相机视图空间 (View Space)
                    let view_pos = view_matrix * corner.extend(1.0);
                    // 在右手坐标系中，相机看向 -Z 轴，所以深度是 -view_pos.z
                    let depth = -view_pos.z;
                    if depth < min_z { min_z = depth; }
                    if depth > max_z { max_z = depth; }
                }

                // 防止除 0
                if max_z - min_z < 1e-5 {
                    max_z = min_z + 1.0;
                }

                // 利用模型自身的 min_z 和 max_z 将深度归一化到 0-255
                let mut h_data = Vec::with_capacity(depth_values.len());
                for &z_raw in depth_values.iter() {
                    if z_raw >= 0.9999 {
                        h_data.push(0);
                    } else {
                        // 对于正交相机，深度 z_raw (0.0 到 1.0) 与真实的线性深度是正向的直接线性关系
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

                save_rgbh(viewport.width, viewport.height, &pixels, &h_data);
                // 添加采样保存深度的调用 (这里步长选 16，保证文本宽度适中)
                save_depth_txt(viewport.width, viewport.height, &h_data, 16);
                println!(">>> RGBH 图片和 txt 深度采样已生成！");
            }
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
            let src_idx = ((h - 1 - y) * w + x) as usize;
            // 0.17 read_color 返回 RGBA 数据
            let r = rgb_raw[src_idx][0];
            let g = rgb_raw[src_idx][1];
            let b = rgb_raw[src_idx][2];
            canvas.put_pixel(x, y, Rgb([r, g, b]));

            let d = h_raw[src_idx];
            canvas.put_pixel(x + w, y, Rgb([d, d, d]));
        }
    }
    canvas.save("output_rgbh.png").unwrap();
}
