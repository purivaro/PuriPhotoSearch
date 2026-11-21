//! แผ่นรวมภาพย่อสำหรับดูด้วยตา (เครื่องมือของนักพัฒนา อ่านไฟล์อย่างเดียว)
//! cargo run --release -p puri-core --example contact_sheet -- <ผล.jpg> <ด้านยาวต่อรูป> <ไฟล์รูป...>
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (out, edge) = (&args[0], args[1].parse::<u32>().unwrap_or(400));
    let imgs: Vec<image::RgbImage> = args[2..]
        .iter()
        .map(|f| {
            let bytes = std::fs::read(f).expect("อ่านรูป");
            let img = puri_core::media::decode_upright(&bytes, edge).expect("ถอดรหัส").to_rgb8();
            image::imageops::thumbnail(&img, edge, edge)
        })
        .collect();
    let (w, h) = (edge * imgs.len() as u32, edge);
    let mut sheet = image::RgbImage::from_pixel(w, h, image::Rgb([20, 20, 20]));
    for (i, img) in imgs.iter().enumerate() {
        let x = i as u32 * edge + (edge - img.width()) / 2;
        image::imageops::replace(&mut sheet, img, i64::from(x), i64::from((edge - img.height()) / 2));
    }
    sheet.save(out).expect("เขียนผล");
}
