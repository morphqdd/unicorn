use cranelift::prelude::types;

#[derive(Debug)]
pub struct TypeDef {
    name: String,
    fields: Vec<Field>,
    size: usize,
    align: usize,
}

impl TypeDef {
    pub fn new(name: &str, fields: Vec<Field>) -> Self {
        let mut ty = Self {
            name: name.to_string(),
            fields,
            size: 0,
            align: 0,
        };
        compute_layout(&mut ty);
        ty
    }
}

#[derive(Debug)]
pub struct Field {
    name: String,
    ty: types::Type,
    offset: usize,
}

impl Field {
    pub fn new(name: &str, ty: types::Type) -> Self {
        Self {
            name: name.to_string(),
            ty,
            offset: 0,
        }
    }
}

fn compute_layout(ty: &mut TypeDef) {
    let mut offset = 0;
    let mut struct_align = 1;

    for field in &mut ty.fields {
        offset = align_up(offset, field.ty.bytes() as usize);
        field.offset = offset;
        offset += field.ty.bytes() as usize;
        struct_align = struct_align.max(field.ty.bytes() as usize);
    }

    ty.size = align_up(offset, struct_align);
    ty.align = struct_align;
}

fn align_up(offset: usize, align: usize) -> usize {
    (offset + align - 1) & !(align - 1)
}
