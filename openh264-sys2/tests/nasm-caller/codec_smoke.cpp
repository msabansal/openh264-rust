#include <cstdio>
#include <vector>

#include "codec_api.h"

int main() {
  constexpr int kWidth = 64;
  constexpr int kHeight = 64;
  constexpr int kFrames = 6;
  ISVCEncoder* encoder = nullptr;
  ISVCDecoder* decoder = nullptr;
  if (WelsCreateSVCEncoder(&encoder) != 0 || encoder == nullptr ||
      WelsCreateDecoder(&decoder) != 0 || decoder == nullptr) {
    return 1;
  }

  SEncParamBase parameters = {};
  parameters.iUsageType = CAMERA_VIDEO_REAL_TIME;
  parameters.iPicWidth = kWidth;
  parameters.iPicHeight = kHeight;
  parameters.iTargetBitrate = 100000;
  parameters.iRCMode = RC_OFF_MODE;
  parameters.fMaxFrameRate = 30;
  if (encoder->Initialize(&parameters) != 0) {
    return 2;
  }
  int format = videoFormatI420;
  if (encoder->SetOption(ENCODER_OPTION_DATAFORMAT, &format) != 0) {
    return 3;
  }
  SDecodingParam decoding = {};
  decoding.sVideoProperty.eVideoBsType = VIDEO_BITSTREAM_AVC;
  decoding.eEcActiveIdc = ERROR_CON_SLICE_COPY;
  if (decoder->Initialize(&decoding) != 0) {
    return 4;
  }

  std::vector<unsigned char> pixels(kWidth * kHeight * 3 / 2, 128);
  SSourcePicture picture = {};
  picture.iColorFormat = videoFormatI420;
  picture.iPicWidth = kWidth;
  picture.iPicHeight = kHeight;
  picture.iStride[0] = kWidth;
  picture.iStride[1] = kWidth / 2;
  picture.iStride[2] = kWidth / 2;
  picture.pData[0] = pixels.data();
  picture.pData[1] = pixels.data() + kWidth * kHeight;
  picture.pData[2] = pixels.data() + kWidth * kHeight * 5 / 4;

  int decoded_frames = 0;
  int total_bytes = 0;
  unsigned long long absolute_error = 0;
  unsigned long long samples = 0;
  for (int frame = 0; frame < kFrames; ++frame) {
    for (int y = 0; y < kHeight; ++y) {
      for (int x = 0; x < kWidth; ++x) {
        pixels[y * kWidth + x] =
            static_cast<unsigned char>(32 + ((x + y + frame * 4) % 160));
      }
    }
    picture.uiTimeStamp = frame * 33;
    SFrameBSInfo bitstream = {};
    if (encoder->EncodeFrame(&picture, &bitstream) != 0 ||
        bitstream.eFrameType == videoFrameTypeSkip) {
      return 5;
    }
    std::vector<unsigned char> encoded;
    for (int layer = 0; layer < bitstream.iLayerNum; ++layer) {
      const auto& info = bitstream.sLayerInfo[layer];
      int length = 0;
      for (int nal = 0; nal < info.iNalCount; ++nal) {
        length += info.pNalLengthInByte[nal];
      }
      encoded.insert(encoded.end(), info.pBsBuf, info.pBsBuf + length);
    }
    if (encoded.empty()) {
      return 6;
    }
    total_bytes += static_cast<int>(encoded.size());
    unsigned char* output[3] = {};
    SBufferInfo info = {};
    if (decoder->DecodeFrameNoDelay(encoded.data(),
                                   static_cast<int>(encoded.size()),
                                   output, &info) != dsErrorFree) {
      return 7;
    }
    const auto& system = info.UsrData.sSystemBuffer;
    if (info.iBufferStatus != 1 || system.iWidth != kWidth ||
        system.iHeight != kHeight || output[0] == nullptr) {
      return 8;
    }
    ++decoded_frames;
    for (int y = 0; y < kHeight; ++y) {
      for (int x = 0; x < kWidth; ++x) {
        int difference =
            output[0][y * system.iStride[0] + x] - pixels[y * kWidth + x];
        absolute_error += difference < 0 ? -difference : difference;
        ++samples;
      }
    }
  }
  encoder->Uninitialize();
  decoder->Uninitialize();
  WelsDestroySVCEncoder(encoder);
  WelsDestroyDecoder(decoder);
  std::printf("Codec smoke: decoded=%d bytes=%d luma_MAE=%.4f\n",
              decoded_frames, total_bytes,
              samples ? static_cast<double>(absolute_error) / samples : 999.0);
  return decoded_frames == kFrames && total_bytes > 0 &&
                 absolute_error < samples * 10
             ? 0
             : 9;
}
