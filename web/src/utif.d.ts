declare module 'utif' {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  type IFD = { width: number; height: number; [tag: string]: any };
  const UTIF: {
    decode(buf: ArrayBuffer): IFD[];
    decodeImage(buf: ArrayBuffer, img: IFD, ifds?: IFD[]): void;
    toRGBA8(img: IFD): Uint8Array;
  };
  export default UTIF;
}
