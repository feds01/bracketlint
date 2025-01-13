#[derive(Debug, Clone, Copy)]
pub struct Dialect {
    pub variant: DialectVariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialectVariant {
    Jinja,
    Liquid,
    Twig,
    Django,
}

impl Dialect {
    pub const fn new(variant: DialectVariant) -> Self {
        match variant {
            DialectVariant::Jinja => Self { variant },
            DialectVariant::Liquid => Self { variant },
            DialectVariant::Twig => Self { variant },
            DialectVariant::Django => Self { variant },
        }
    }

    /// Check if its currently Django dialect.
    #[inline]
    pub fn is_django(&self) -> bool {
        self.variant == DialectVariant::Django
    }
}

impl Default for Dialect {
    fn default() -> Self {
        Self::new(DialectVariant::Django)
    }
}
