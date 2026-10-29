//! Baked display defaults plus runtime-only Time Machine display controls.
//! No filesystem-backed settings are required at startup.

pub const MATHEMATICAL_STAR_MULTIPLIER:f64=1.0;
pub const DEFAULT_STAR_MULTIPLIER:f64=3.0;
pub const MAX_STAR_MULTIPLIER:f64=30.0;

#[derive(Debug,Clone)]
pub struct DisplaySettings {
    star_multiplier:f64,
}
impl Default for DisplaySettings {
    fn default()->Self {Self{star_multiplier:DEFAULT_STAR_MULTIPLIER}}
}
impl DisplaySettings {
    pub fn star_multiplier(&self)->f64 {self.star_multiplier}
    pub fn set_star_multiplier(&mut self,multiplier:f64)->Result<(),String> {
        if !multiplier.is_finite() || multiplier<=0.0 || multiplier>MAX_STAR_MULTIPLIER {
            return Err(format!("star brightness multiplier must be finite, positive, and at most {MAX_STAR_MULTIPLIER:.0}"));
        }
        self.star_multiplier=multiplier;
        Ok(())
    }
}

pub fn format_multiplier(value:f64)->String {
    if value.fract()==0.0 {format!("{value:.0}")} else {value.to_string()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baked_default_and_runtime_validation_are_preserved() {
        let mut settings=DisplaySettings::default();
        assert_eq!(MATHEMATICAL_STAR_MULTIPLIER,1.0);
        assert_eq!(settings.star_multiplier(),3.0);
        assert!(settings.set_star_multiplier(30.0).is_ok());
        assert_eq!(settings.star_multiplier(),30.0);
        for value in [0.0,-1.0,30.0001,31.0,f64::NAN,f64::INFINITY] {
            assert!(settings.set_star_multiplier(value).is_err(),"value={value}");
            assert_eq!(settings.star_multiplier(),30.0,"invalid values must preserve the active setting");
        }
        settings.set_star_multiplier(4.0).unwrap();
        assert_eq!(settings.star_multiplier(),4.0);
    }
}
