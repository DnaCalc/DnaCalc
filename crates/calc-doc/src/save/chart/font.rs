//! Edit known chart and element text properties while retaining unrelated DrawingML.
use super::*;
use calc_doc_model::ChartTextStyle;

fn font_xml(style:&ChartTextStyle,kind:&str)->Vec<u8>{
    let mut out=format!("<a:{kind} xmlns:a=\"{A}\"").into_bytes();
    if let Some(size)=style.size{out.extend(format!(" sz=\"{size}\"").as_bytes());}
    for (key,value) in [("b",style.bold),("i",style.italic)]{if let Some(value)=value{out.extend(format!(" {key}=\"{}\"",u8::from(value)).as_bytes());}}
    out.push(b'>');out.extend(colour::fill(style.color.as_deref()));
    if let Some(family)=&style.family{out.extend(format!("<a:latin xmlns:a=\"{A}\" typeface=").as_bytes());super::super::text::attribute_value(family,&mut out);out.extend(b"/>");}
    out.extend(format!("</a:{kind}>").as_bytes());out
}
pub(super) fn fresh(style:&ChartTextStyle)->Vec<u8>{
    let mut out=format!("<c:txPr xmlns:c=\"{C}\" xmlns:a=\"{A}\"><a:bodyPr/><a:lstStyle/><a:p><a:pPr>").into_bytes();out.extend(font_xml(style,"defRPr"));out.extend(b"</a:pPr></a:p></c:txPr>");out
}
fn rebuild(xml:&XmlPart,span:(usize,usize),end:usize,mut patches:Vec<Patch>)->Vec<u8>{
    patches.sort_by_key(|(s,_)|*s);let mut splice=Splice::new(&xml.bytes()[span.0..end]);for ((a,b),bytes) in patches{splice.replace((a-span.0,b-span.0),&bytes);}splice.finish()
}
fn expand(tag:&RawTag<'_>,body:Vec<u8>)->Vec<u8>{let mut out=tag.rebuilt(&[],false);out.extend(body);out.extend(b"</");out.extend(tag.qualified_name());out.push(b'>');out}

/// Only known text containers are entered; extension subtrees keep their bytes.
pub(super) fn replace(xml:&XmlPart,reader:&mut Reader<'_>,span:(usize,usize),kind:&str,style:&ChartTextStyle,before:&ChartTextStyle)->Result<(usize,Vec<u8>),SaveRefusal>{
    let tag=RawTag::parse(&xml.bytes()[span.0..span.1]);let mut children=Vec::new();let mut patches=Vec::new();
    let (close,end)=loop{match reader.expect()?{
        (XmlEvent::Start(e),s)=>{
            let local=e.name.local.as_str();let known=e.name.namespace.as_deref()==Some(A)&&match kind{"txPr"=>local=="p","p"=>matches!(local,"pPr"|"r"|"endParaRPr"),"pPr"=>local=="defRPr","r"=>local=="rPr",_=>false};
            let end=if known{
                let (end,bytes)=if matches!(local,"rPr"|"defRPr"|"endParaRPr"){run(xml,reader,s,style,before)?}else{replace(xml,reader,s,local,style,before)?};patches.push(((s.0,end),bytes));end
            }else{reader.element_end()?};children.push((e.name,(s.0,end)));
        },(XmlEvent::End(_),s)=>break s,_=>{}
    }};
    let delta=ChartTextStyle{family:if style.family!=before.family{style.family.clone()}else{None},size:if style.size!=before.size{style.size}else{None},bold:if style.bold!=before.bold{style.bold}else{None},italic:if style.italic!=before.italic{style.italic}else{None},color:if style.color!=before.color{style.color.clone()}else{None}};
    let style=&delta;
    let has=|name:&str|children.iter().any(|(n,_)|n.is(A,name));
    let missing=match kind{
        "txPr" if !has("p")=>{let mut b=b"<a:p xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:pPr>".to_vec();b.extend(font_xml(style,"defRPr"));b.extend(b"</a:pPr></a:p>");Some((close,b))},
        "p" if !has("pPr")=>{let mut b=format!("<a:pPr xmlns:a=\"{A}\">").into_bytes();b.extend(font_xml(style,"defRPr"));b.extend(b"</a:pPr>");Some((span.1,b))},
        "pPr" if !has("defRPr")=>Some((children.iter().find(|(n,_)|n.is(A,"extLst")).map_or(close,|(_,s)|s.0),font_xml(style,"defRPr"))),
        "r" if !has("rPr")=>Some((span.1,font_xml(style,"rPr"))),_=>None,
    };
    if let Some((at,body))=missing{if tag.is_empty(){return Ok((end,expand(&tag,body)));}patches.push(((at,at),body));}
    Ok((end,rebuild(xml,span,end,patches)))
}
fn run(xml:&XmlPart,reader:&mut Reader<'_>,span:(usize,usize),style:&ChartTextStyle,before:&ChartTextStyle)->Result<(usize,Vec<u8>),SaveRefusal>{
    let tag=RawTag::parse(&xml.bytes()[span.0..span.1]);let mut children=Vec::new();let (close,end)=loop{match reader.expect()?{
        (XmlEvent::Start(e),s)=>{let end=reader.element_end()?;children.push((e.name,(s.0,end),s));},(XmlEvent::End(_),s)=>break s,_=>{}
    }};
    let fills=children.iter().filter(|(n,_,_)|n.namespace.as_deref()==Some(A)&&matches!(n.local.as_str(),"solidFill"|"noFill"|"gradFill"|"blipFill"|"pattFill"|"grpFill")).collect::<Vec<_>>();
    let latin=children.iter().filter(|(n,_,_)|n.is(A,"latin")).collect::<Vec<_>>();
    if fills.len()>1||latin.len()>1{return Err(reader.invalid("Duplicate chart text fill/font records (dc-971p.59)"));}
    let size=style.size.map(|n|n.to_string());let boolean=|v:Option<bool>|v.map(|v|if v{"1"}else{"0"});
    let mut attributes=Vec::new();if style.size!=before.size{attributes.push(("sz",size.as_deref()));}if style.bold!=before.bold{attributes.push(("b",boolean(style.bold)));}if style.italic!=before.italic{attributes.push(("i",boolean(style.italic)));}
    let mut patches=vec![(span,tag.rebuilt(&attributes,tag.is_empty()))];
    let mut inserts=std::collections::BTreeMap::<usize,Vec<u8>>::new();
    if style.color!=before.color {if let Some((_,s,_))=fills.first(){patches.push((*s,colour::fill(style.color.as_deref())));}else{
        let at=children.iter().find(|(n,_,_)|!n.is(A,"ln")).map_or(close,|(_,s,_)|s.0);inserts.entry(at).or_default().extend(colour::fill(style.color.as_deref()));
    }
    }
    if style.family!=before.family {if let Some((_,full,head))=latin.first(){
        if let Some(family)=&style.family{let old=RawTag::parse(&xml.bytes()[head.0..head.1]);patches.push((*head,old.rebuilt(&[("typeface",Some(family))],old.is_empty())));}else{patches.push((*full,Vec::new()));}
    }else if let Some(family)=&style.family{
        let at=children.iter().find(|(n,_,_)|n.namespace.as_deref()==Some(A)&&matches!(n.local.as_str(),"ea"|"cs"|"sym"|"hlinkClick"|"hlinkMouseOver"|"rtl"|"extLst")).map_or(close,|(_,s,_)|s.0);
        let b=inserts.entry(at).or_default();b.extend(format!("<a:latin xmlns:a=\"{A}\" typeface=").as_bytes());super::super::text::attribute_value(family,b);b.extend(b"/>");
    }
    }
    if tag.is_empty(){
        let mut out=tag.rebuilt(&attributes,false);for b in inserts.values(){out.extend(b);}out.extend(b"</");out.extend(tag.qualified_name());out.push(b'>');return Ok((end,out));
    }
    for (at,b) in inserts{if !b.is_empty(){patches.push(((at,at),b));}}
    Ok((end,rebuild(xml,span,end,patches)))
}

pub(super) fn apply(xml:&XmlPart,before:&ChartTextStyle,style:&ChartTextStyle,patches:&mut Vec<Patch>)->Result<(),SaveRefusal>{
    let mut changed=style.clone();if style.color==before.color{changed.color=None;}validate(&changed).map_err(|s|SaveRefusal::ChartNotModelled{part:xml.name().into(),detail:s.into(),owner:"dc-971p.59"})?;
    let mut reader=Reader::new(xml);reader.expect()?;let mut found=false;let mut late=None;let close=loop{match reader.expect()?{
        (XmlEvent::Start(e),span) if e.name.is(C,"txPr")=>{
            if found{return Err(reader.invalid("Duplicate chart-space text properties (dc-971p.59)"));}found=true;let (end,bytes)=replace(xml,&mut reader,span,"txPr",style,before)?;patches.push(((span.0,end),bytes));
        },
        (XmlEvent::Start(e),span)=>{if e.name.namespace.as_deref()==Some(C)&&matches!(e.name.local.as_str(),"externalData"|"printSettings"|"userShapes"|"extLst"){late.get_or_insert(span.0);}reader.element_end()?;},
        (XmlEvent::End(_),span)=>break span.0,_=>{},
    }};reader.end()?;if !found{let at=late.unwrap_or(close);patches.push(((at,at),fresh(style)));}Ok(())
}
pub(crate) fn validate(style:&ChartTextStyle)->Result<(),&'static str>{
    if style.size.is_some_and(|n|!(100..=40900).contains(&n)){return Err("Use a chart font size from 1 to 409 points (dc-971p.59)");}
    if style.family.as_ref().is_some_and(|s|s.is_empty()||s.encode_utf16().count()>255||s.chars().any(|c|c<' '||matches!(c,'\u{fffe}'|'\u{ffff}'))){return Err("Use a font family of 1 to 255 characters without control characters (dc-971p.59)");}
    if style.color.as_ref().is_some_and(|s|s.len()!=6||!s.bytes().all(|c|c.is_ascii_hexdigit())){return Err("Use a six-digit RGB text colour or Automatic (dc-971p.59)");}Ok(())
}

#[cfg(test)]
mod tests {
 use super::*;
 #[cfg_attr(not(target_arch="wasm32"),test)]
 #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
 fn retained_text_runs_keep_unknown_attributes_children_and_schema_order(){
  let body=r#"<a:bodyPr rot="60000"/><a:lstStyle/><a:p><a:pPr algn="ctr"><a:defRPr lang="en-ZA" sz="1100"><a:ln w="10"/><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:effectLst/><a:latin typeface="Old" pitchFamily="34"/><a:ea typeface="Retained"/><a:extLst><a:ext uri="keep"/></a:extLst></a:defRPr></a:pPr><a:r><a:rPr lang="en-US"/><a:t>Preserved text</a:t></a:r><a:endParaRPr/></a:p><c:extLst><c:ext uri="keep-root"><a:latin typeface="Extension"/></c:ext></c:extLst>"#;
  let xml=XmlPart::new("chart.xml".into(),format!(r#"<c:txPr xmlns:c="{C}" xmlns:a="{A}">{body}</c:txPr>"#).into_bytes(),crate::xml::XmlLimits::default());let mut reader=Reader::new(&xml);let (_,span)=reader.expect().unwrap();let style=ChartTextStyle{family:Some("Arial & Test".into()),size:Some(1350),bold:Some(true),italic:Some(true),color:Some("123ABC".into())};let (_,bytes)=replace(&xml,&mut reader,span,"txPr",&style,&ChartTextStyle::default()).unwrap();let text=String::from_utf8(bytes).unwrap();
  for retained in ["rot=\"60000\"","algn=\"ctr\"","lang=\"en-ZA\"","pitchFamily=\"34\"","<a:ln w=\"10\"/>","<a:effectLst/>","<a:ea typeface=\"Retained\"/>","<a:ext uri=\"keep\"/>","<a:t>Preserved text</a:t>","<a:latin typeface=\"Extension\"/>"]{assert!(text.contains(retained),"{retained}: {text}");}
  assert!(text.contains("Arial &amp; Test"));assert!(text.contains("sz=\"1350\""));assert!(text.contains("b=\"1\""));assert!(text.contains("i=\"1\""));assert_eq!(text.matches("123ABC").count(),3);assert!(text.find("solidFill").unwrap()<text.find("effectLst").unwrap());assert!(text.find("effectLst").unwrap()<text.find("<a:latin").unwrap());
  let xml=xml.with_bytes(text.into_bytes());let mut reader=Reader::new(&xml);let (_,span)=reader.expect().unwrap();let (_,bytes)=replace(&xml,&mut reader,span,"txPr",&ChartTextStyle::default(),&style).unwrap();let text=String::from_utf8(bytes).unwrap();assert!(!text.contains("123ABC")&&!text.contains("1350")&&!text.contains("Arial"));assert!(text.contains("Extension")&&text.contains("Retained")&&text.contains("Preserved text"));
 }
}

#[cfg(test)]
mod partial_tests {
 use super::*;
 #[cfg_attr(not(target_arch="wasm32"),test)]
 #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
 fn a_family_change_keeps_the_authored_theme_fill_and_transform(){
  let fill=r#"<a:solidFill><a:schemeClr val="accent2"><a:lumMod val="50000"/></a:schemeClr></a:solidFill>"#;
  let xml=XmlPart::new("chart.xml".into(),format!(r#"<c:txPr xmlns:c="{C}" xmlns:a="{A}"><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="1100">{fill}</a:defRPr></a:pPr></a:p></c:txPr>"#).into_bytes(),crate::xml::XmlLimits::default());let before=ChartTextStyle{size:Some(1100),color:Some("accent2".into()),..Default::default()};let mut after=before.clone();after.family=Some("Arial".into());let mut reader=Reader::new(&xml);let (_,span)=reader.expect().unwrap();let (_,bytes)=replace(&xml,&mut reader,span,"txPr",&after,&before).unwrap();let text=String::from_utf8(bytes).unwrap();assert!(text.contains(fill),"{text}");assert!(text.contains("typeface=\"Arial\"")&&text.contains("sz=\"1100\""));
 }
}

/// Locate direct chart title/legend and plot-area axis objects by their authored identity.
fn element_span(xml:&XmlPart,target:&str)->Result<Option<(usize,usize)>,SaveRefusal>{
    let mut reader=Reader::new(xml);let mut frames=Vec::<(String,usize,Option<String>,Option<u32>)>::new();let mut found=None;
    while let Some((event,span))=reader.next()?{match event{
        XmlEvent::Start(e)=>{
            let chart=e.name.namespace.as_deref()==Some(C);let path=frames.iter().map(|f|f.0.as_str()).collect::<Vec<_>>();
            let key=if chart&&path==["chartSpace","chart"]&&matches!(e.name.local.as_str(),"title"|"legend"){Some(e.name.local.to_string())}else if chart&&path==["chartSpace","chart","plotArea"]&&matches!(e.name.local.as_str(),"valAx"|"catAx"|"dateAx"|"serAx"){Some("axis:".into())}else{None};
            if chart&&e.name.local=="axId"{if let Some(frame)=frames.last_mut().filter(|f|f.2.as_deref()==Some("axis:")){frame.3=e.attribute("val").and_then(|s|s.parse().ok());}}
            frames.push((if chart{e.name.local.to_string()}else{String::new()},span.0,key,None));
        },XmlEvent::End(_)=>{let Some((_,start,key,id))=frames.pop()else{return Err(reader.invalid("Unbalanced chart text container"));};if let Some(mut key)=key{if key=="axis:"{let Some(id)=id else{continue;};key.push_str(&id.to_string());}if key==target{if found.replace((start,span.1)).is_some(){return Err(reader.invalid("Duplicate chart text target"));}}}},XmlEvent::Text(_)=>{}
    }}reader.end()?;Ok(found)
}
fn object_text(xml:&XmlPart,reader:&mut Reader<'_>,span:(usize,usize),kind:&str,style:&ChartTextStyle,before:&ChartTextStyle)->Result<(usize,Vec<u8>),SaveRefusal>{
    let tag=RawTag::parse(&xml.bytes()[span.0..span.1]);let mut patches=Vec::new();let mut found=false;let mut late=None;
    let (close,end)=loop{match reader.expect()?{
        (XmlEvent::Start(e),child)=>{
            let end=if e.name.is(C,"txPr")&&kind!="tx"{
                if found{return Err(reader.invalid("Duplicate object text properties"));}found=true;let (end,bytes)=replace(xml,reader,child,"txPr",style,before)?;patches.push(((child.0,end),bytes));end
            }else if kind=="title"&&e.name.is(C,"tx"){
                let (end,bytes)=object_text(xml,reader,child,"tx",style,before)?;patches.push(((child.0,end),bytes));end
            }else if kind=="tx"&&e.name.is(C,"rich"){
                let (end,bytes)=replace(xml,reader,child,"txPr",style,before)?;patches.push(((child.0,end),bytes));end
            }else{reader.element_end()?};
            let after_text=if kind.ends_with("Ax"){matches!(e.name.local.as_str(),"crossAx"|"crosses"|"crossesAt"|"crossBetween"|"majorUnit"|"minorUnit"|"dispUnits"|"auto"|"lblAlgn"|"lblOffset"|"tickLblSkip"|"tickMarkSkip"|"noMultiLvlLbl"|"extLst")}else{e.name.local=="extLst"};
            if e.name.namespace.as_deref()==Some(C)&&after_text{late.get_or_insert(child.0);}let _=end;
        },(XmlEvent::End(_),s)=>break s,_=>{}
    }};
    if !found&&kind!="tx"&&*style!=ChartTextStyle::default(){let delta=ChartTextStyle{family:if style.family!=before.family{style.family.clone()}else{None},size:if style.size!=before.size{style.size}else{None},bold:if style.bold!=before.bold{style.bold}else{None},italic:if style.italic!=before.italic{style.italic}else{None},color:if style.color!=before.color{style.color.clone()}else{None}};let bytes=fresh(&delta);if tag.is_empty(){return Ok((end,expand(&tag,bytes)));}let at=late.unwrap_or(close);patches.push(((at,at),bytes));}
    Ok((end,rebuild(xml,span,end,patches)))
}
/// Object text edits use the same run writer as chart defaults, after other chart edits.
pub(super) fn apply_elements(xml:&XmlPart,before:&std::collections::BTreeMap<String,ChartTextStyle>,after:&std::collections::BTreeMap<String,ChartTextStyle>)->Result<Vec<u8>,SaveRefusal>{
    let mut bytes=xml.bytes().to_vec();let keys=before.keys().chain(after.keys()).collect::<std::collections::BTreeSet<_>>();
    for target in keys{
        let empty=ChartTextStyle::default();let old=before.get(target).unwrap_or(&empty);let new=after.get(target).unwrap_or(&empty);if old==new{continue;}
        let mut changed=new.clone();if old.color==new.color{changed.color=None;}validate(&changed).map_err(|detail|SaveRefusal::ChartNotModelled{part:xml.name().into(),detail:detail.into(),owner:"dc-971p.61"})?;
        let current=xml.with_bytes(bytes);let Some(wanted)=element_span(&current,target)?else{if after.contains_key(target){return Err(SaveRefusal::ChartNotModelled{part:xml.name().into(),detail:"The selected chart text object is absent".into(),owner:"dc-971p.61"});}bytes=current.bytes().to_vec();continue;};
        let mut reader=Reader::new(&current);let mut patch=None;while let Some((event,span))=reader.next()?{if span.0==wanted.0{let XmlEvent::Start(e)=event else{return Err(reader.invalid("Chart text target is not an element"));};let (end,body)=object_text(&current,&mut reader,span,&e.name.local,new,old)?;patch=Some(((span.0,end),body));break;}}
        let (span,body)=patch.ok_or_else(||reader.invalid("Chart text target disappeared"))?;let mut splice=Splice::new(current.bytes());splice.replace(span,&body);bytes=splice.finish();
    }Ok(bytes)
}
