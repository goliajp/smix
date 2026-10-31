import CoreGraphics

extension TreeRoute.A11ySnapshotData {
  /// The application node with its frame in the shape the app is laid out.
  ///
  /// On an unfolded foldable the application's frame comes back in the
  /// inner panel's own portrait shape (669 x 951) while a landscape app's
  /// elements are placed in a space 951 wide. Judged against that frame,
  /// everything right of x = 669 read as off screen: not visible in the
  /// tree, refused when asked for by name. Only the root's frame changes;
  /// the element frames were right all along.
  public func inInterfaceShape(landscape: Bool) -> TreeRoute.A11ySnapshotData {
    let tall = frame.height > frame.width
    guard landscape == tall else { return self }
    return TreeRoute.A11ySnapshotData(
      elementTypeRawValue: elementTypeRawValue,
      identifier: identifier,
      label: label,
      value: value,
      frame: CGRect(
        x: frame.origin.x, y: frame.origin.y, width: frame.height, height: frame.width),
      isEnabled: isEnabled,
      isSelected: isSelected,
      hasFocus: hasFocus,
      children: children,
      title: title,
      placeholderValue: placeholderValue
    )
  }
}
