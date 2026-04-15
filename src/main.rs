use three_d::*;

fn main() {
    let window = Window::new(WindowSettings {
        title: "RGBH Capture Tool".to_string(),
        max_size: Some((1280, 720)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    // 1. 初始化相机
    let mut camera = Camera::new_perspective(
        window.viewport(),
        vec3(3.0, 3.0, 5.0),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        20.0,
    );
    let mut control = OrbitControl::new(*camera.target(), 1.0, 50.0);

    // 2. 创建模型 (使用 Gm 和 Mesh)
    let mesh = Mesh::new(&context, &CpuMesh::sphere(32));
    let material = PhysicalMaterial::new_opaque(
        &context,
        &CpuMaterial {
            albedo: Srgba::new_opaque(0, 150, 255),
            ..Default::default()
        },
    );
    let model = Gm::new(mesh, material);

    let ambient = AmbientLight::new(&context, 0.5, Srgba::WHITE);
    let directional = DirectionalLight::new(&context, 2.0, Srgba::WHITE, &vec3(-1.0, -1.0, -1.0));

    // 3. 渲染循环
    window.render_loop(move |mut frame_input| {
        let viewport = frame_input.viewport;
        camera.set_viewport(viewport);
        control.handle_events(&mut camera, &mut frame_input.events);

        // 渲染到屏幕
        let screen = frame_input.screen();
        screen
            .clear(ClearState::color_and_depth(0.1, 0.1, 0.1, 1.0, 1.0))
            .render(&camera, &model, &[&ambient, &directional]);

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
                let mut h_data = Vec::with_capacity(depth_values.len());

                for &z_raw in depth_values.iter() {
                    // GPU 深度还原公式
                    let z_ndc = z_raw * 2.0 - 1.0;
                    let z_linear = (2.0 * near * far) / (far + near - z_ndc * (far - near));

                    // 映射到 0-255 (近处白 255，远处黑 0)
                    let h_val =
                        (255.0 * (1.0 - (z_linear - near) / (far - near))).clamp(0.0, 255.0) as u8;
                    h_data.push(h_val);
                }

                save_rgbh(viewport.width, viewport.height, &pixels, &h_data);
                println!(">>> RGBH 图片已生成！");
            }
        }
        FrameOutput::default()
    });
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
