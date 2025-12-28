use ratatui::{
  layout::Rect,
  style::{
    Color,
    Style,
  },
  text::{
    Line,
    Span,
  },
  widgets::{
    Block,
    Borders,
    Paragraph,
  },
};
use std::path::Path;

type ImageProto = ratatui_image::protocol::StatefulProtocol;

pub fn draw_image_preview(
  f: &mut ratatui::Frame,
  area: Rect,
  app: &mut crate::App,
  path: &Path,
)
{
  let mut block = Block::default().borders(Borders::ALL);
  if let Some(th) = app.config.ui.theme.as_ref()
  {
    if let Some(bg) =
      th.pane_bg.as_ref().and_then(|s| crate::ui::colors::parse_color(s))
    {
      block = block.style(Style::default().bg(bg));
    }
    if let Some(bfg) =
      th.border_fg.as_ref().and_then(|s| crate::ui::colors::parse_color(s))
    {
      block = block.border_style(Style::default().fg(bfg));
    }
  }
  
  let inner = block.inner(area);
  f.render_widget(block, area);
  
  match image::open(path)
  {
    Ok(dyn_img) =>
    {
      if app.image_state.is_none()
      {
        match init_image_protocol(dyn_img.clone())
        {
          Ok(proto) => app.image_state = Some(Box::new(proto)),
          Err(e) =>
          {
            crate::trace::log(format!("[image] protocol init failed: {}", e));
            let text = vec![
              Line::from(Span::styled(
                "Image protocol unavailable",
                Style::default().fg(Color::Yellow),
              )),
              Line::from(Span::styled(
                format!("Error: {}", e),
                Style::default().fg(Color::Gray),
              )),
            ];
            let para = Paragraph::new(text);
            f.render_widget(para, inner);
            return;
          }
        }
      }
      
      if let Some(state) = app.image_state.as_mut()
      {
        if let Some(proto) = state.downcast_mut::<ImageProto>()
        {
          use ratatui_image::StatefulImage;
          let img = StatefulImage::new();
          f.render_stateful_widget(img, inner, proto);
        }
      }
      else
      {
        let text = vec![
          Line::from(Span::styled(
            "Image preview unavailable",
            Style::default().fg(Color::Yellow),
          )),
          Line::from(Span::styled(
            format!("File: {}", path.display()),
            Style::default().fg(Color::Gray),
          )),
        ];
        let para = Paragraph::new(text);
        f.render_widget(para, inner);
      }
    }
    Err(e) =>
    {
      let text = vec![
        Line::from(Span::styled(
          "Failed to load image",
          Style::default().fg(Color::Red),
        )),
        Line::from(Span::styled(
          format!("Error: {}", e),
          Style::default().fg(Color::Gray),
        )),
      ];
      let para = Paragraph::new(text);
      f.render_widget(para, inner);
    }
  }
}

fn init_image_protocol(
  img: image::DynamicImage,
) -> Result<ImageProto, Box<dyn std::error::Error>>
{
  use ratatui_image::picker::Picker;
  
  let picker = match Picker::from_query_stdio() {
    Ok(p) => {
      crate::trace::log(format!("[image] auto-detected protocol"));
      p
    },
    Err(e) => {
      crate::trace::log(format!("[image] protocol detection failed: {}, using halfblocks", e));
      Picker::halfblocks()
    }
  };
  
  let proto = picker.new_resize_protocol(img);
  Ok(proto)
}
