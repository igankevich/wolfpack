use std::fmt::Debug;

use arbitrary::Arbitrary;
use arbtest::arbtest;

//use crate::rpm::EntryIo;
use crate::rpm::ValueIo;

pub fn write_read_symmetry<T: Debug + for<'a> Arbitrary<'a> + ValueIo + Eq>() {
    arbtest(|u| {
        let expected: T = u.arbitrary()?;
        let mut buf = Vec::new();
        expected.write(&mut buf).unwrap();
        let actual = T::read(&buf, expected.count())
            .map_err(|e| {
                panic!("{e}, type = {}", std::any::type_name::<T>());
            })
            .unwrap();
        assert_eq!(expected, actual, "type = {}", std::any::type_name::<T>());
        Ok(())
    });
}

/*
 * TODO
pub fn write_read_entry_symmetry<T: Debug + for<'a> Arbitrary<'a> + EntryIo + Eq>() {
    arbtest(|u| {
        let expected: T = u.arbitrary()?;
        let mut index = Vec::new();
        let mut store = Vec::new();
        expected.write(&mut index, &mut store, 0).unwrap();
        let actual = T::read(&index, &store)
            .map_err(|e| {
                panic!("{e}, type = {}", std::any::type_name::<T>());
            })
            .unwrap()
            .unwrap();
        assert_eq!(expected, actual, "type = {}", std::any::type_name::<T>());
        Ok(())
    });
}
*/
