package Invalid {
    part def Thing {
        attribute cylinders: ScalarValues::NotARealType = 3
    // deliberately missing closing braces and a semicolon to check the oracle rejects it
